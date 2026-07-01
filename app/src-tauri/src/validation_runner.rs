//! ValidationRunner en vivo — primer consumidor del runtime determinista.
//!
//! Ejecuta un programa de validación (p.ej. `npm test`) DENTRO del sandbox de
//! Codex (`ValidatedCodexRuntime`), sobre un worktree DESECHABLE creado desde un
//! candidate commit inmutable, y verifica de forma determinista:
//!   - exit code REAL del proceso (sin modelo de por medio),
//!   - que HEAD no cambió (el test no debe commitear),
//!   - que no se tocaron fuentes fuera de las rutas generadas permitidas.
//! Entrada: ejecutable + argv (NUNCA una cadena de shell). Conserva el worktree
//! como evidencia si la validación no aprueba.

use crate::codex_runtime::ValidatedCodexRuntime;
use crate::git::{self, ChangedFile};
use crate::validation::ValidationSpec;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wait_timeout::ChildExt;

/// Programa a validar: ejecutable absoluto + argumentos. Sin shell.
#[derive(Debug, Clone)]
pub struct SandboxedProgram {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ValidationOutcome {
    Passed,
    Failed,
    /// El proceso cambió HEAD o tocó fuentes fuera de lo permitido.
    PolicyViolation,
    /// Excedió el tiempo; el proceso fue terminado.
    TimedOut,
    /// No hay runtime de sandbox operativo en esta máquina.
    Unavailable,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationEvidence {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub head_changed: bool,
    pub changed: Vec<ChangedFile>,
    pub timed_out: bool,
}

/// Decisión pura a partir de la evidencia observada (testeable sin sandbox).
pub fn classify(
    timed_out: bool,
    exit_code: Option<i32>,
    head_changed: bool,
    unexpected_changes: bool,
) -> ValidationOutcome {
    if timed_out {
        return ValidationOutcome::TimedOut;
    }
    // El test NO debe commitear ni mover ramas.
    if head_changed {
        return ValidationOutcome::PolicyViolation;
    }
    match exit_code {
        Some(0) => {
            if unexpected_changes {
                ValidationOutcome::PolicyViolation
            } else {
                ValidationOutcome::Passed
            }
        }
        Some(_) => ValidationOutcome::Failed,
        // Sin exit code (proceso caído/matado y no por timeout): no aprobar.
        None => ValidationOutcome::Failed,
    }
}

/// Ejecuta la validación de un candidate commit en un worktree desechable.
/// `repo_root` es la raíz del repo; `candidate_commit` el snapshot inmutable a
/// validar; `wt_path` la ruta (fuera del repo) del worktree desechable.
#[allow(clippy::too_many_arguments)]
pub fn run(
    validated: &ValidatedCodexRuntime,
    repo_root: &Path,
    candidate_commit: &str,
    wt_path: &Path,
    branch: &str,
    program: &SandboxedProgram,
    spec: &ValidationSpec,
    timeout: Duration,
) -> Result<(ValidationOutcome, ValidationEvidence), String> {
    // Worktree desechable desde el candidate commit (snapshot inmutable).
    git::add_worktree(repo_root, wt_path, branch, candidate_commit)?;

    // deny_roots para el PATH filtrado: repo y worktree de validación.
    let deny: Vec<&Path> = vec![repo_root, wt_path];
    let temp_dir = wt_path.join(".nexora-tmp");

    let args: Vec<&str> = program.args.iter().map(|s| s.as_str()).collect();
    let mut child = validated
        .runtime
        .command(
            validated.mode,
            &program.executable,
            &args,
            wt_path,
            &temp_dir,
            &deny,
        )?
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar la validación: {e}"))?;

    let (timed_out, exit_code) = match child
        .wait_timeout(timeout)
        .map_err(|e| format!("wait_timeout: {e}"))?
    {
        Some(status) => (false, status.code()),
        None => {
            // Timeout: matar el ÁRBOL de procesos (codex + hijos) y recoger.
            kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            (true, None)
        }
    };

    let (stdout, stderr) = read_child_output(&mut child);

    // Verificación determinista del estado git tras la ejecución.
    let head_after = git::head_commit(wt_path).unwrap_or_default();
    let head_changed = head_after != candidate_commit;
    let changed = git::changed_files(wt_path).unwrap_or_default();
    let unexpected = spec.has_unexpected_changes(&changed);

    let outcome = classify(timed_out, exit_code, head_changed, unexpected);

    // Conservar el worktree como evidencia si NO aprobó; limpiar si Passed.
    if outcome == ValidationOutcome::Passed {
        let _ = git::remove_worktree(repo_root, wt_path);
    }

    Ok((
        outcome,
        ValidationEvidence { exit_code, stdout, stderr, head_changed, changed, timed_out },
    ))
}

fn read_child_output(child: &mut std::process::Child) -> (String, String) {
    use std::io::Read;
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    (out, err)
}

/// Mata el árbol de procesos por PID. En Windows `taskkill /T /F` termina hijos.
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_covers_all_outcomes() {
        // exit 0 limpio -> Passed
        assert_eq!(classify(false, Some(0), false, false), ValidationOutcome::Passed);
        // exit != 0 -> Failed
        assert_eq!(classify(false, Some(1), false, false), ValidationOutcome::Failed);
        // HEAD cambiado (intento de commit) -> PolicyViolation
        assert_eq!(classify(false, Some(0), true, false), ValidationOutcome::PolicyViolation);
        // exit 0 pero tocó fuentes -> PolicyViolation
        assert_eq!(classify(false, Some(0), false, true), ValidationOutcome::PolicyViolation);
        // timeout -> TimedOut (gana sobre todo)
        assert_eq!(classify(true, None, true, true), ValidationOutcome::TimedOut);
        // sin exit code y sin timeout -> Failed
        assert_eq!(classify(false, None, false, false), ValidationOutcome::Failed);
    }

    // --- Canarios EN VIVO con el sandbox real (ignorados por defecto).
    //     cargo test canary_ -- --ignored --nocapture
    use crate::codex_runtime;

    fn setup() -> Option<(codex_runtime::ValidatedCodexRuntime, PathBuf, String)> {
        let v = codex_runtime::discover()?;
        // repo temporal con un commit base
        let repo = std::env::temp_dir().join(format!("nexora_vr_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).ok()?;
        let run = |args: &[&str]| {
            std::process::Command::new("git").current_dir(&repo).args(args).output().ok()
        };
        run(&["init", "-q", "-b", "main"])?;
        run(&["config", "user.name", "T"])?;
        run(&["config", "user.email", "t@t"])?;
        std::fs::write(repo.join("app.js"), "console.log('hi')").ok()?;
        run(&["add", "-A"])?;
        run(&["commit", "-q", "-m", "base"])?;
        let base = git::head_commit(&repo).ok()?;
        Some((v, repo, base))
    }

    fn spec() -> ValidationSpec {
        ValidationSpec::new("cmd", 60, &["coverage/**".to_string()]).unwrap()
    }

    #[test]
    #[ignore]
    fn canary_exit_zero_clean_passes() {
        let Some((v, repo, base)) = setup() else {
            eprintln!("sin runtime/git: se salta");
            return;
        };
        let wt = std::env::temp_dir().join(format!("nexora_vwt_{}", uuid::Uuid::new_v4()));
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let prog = SandboxedProgram {
            executable: PathBuf::from(format!("{windir}\\System32\\cmd.exe")),
            args: vec!["/c".into(), "echo".into(), "ok".into()],
        };
        let (outcome, ev) = run(&v, &repo, &base, &wt, "nexora/val/ok", &prog, &spec(), Duration::from_secs(60)).unwrap();
        eprintln!("outcome={outcome:?} exit={:?}", ev.exit_code);
        assert_eq!(outcome, ValidationOutcome::Passed);
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    #[ignore]
    fn canary_nonzero_exit_fails() {
        let Some((v, repo, base)) = setup() else { return };
        let wt = std::env::temp_dir().join(format!("nexora_vwt_{}", uuid::Uuid::new_v4()));
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let prog = SandboxedProgram {
            executable: PathBuf::from(format!("{windir}\\System32\\cmd.exe")),
            args: vec!["/c".into(), "exit".into(), "3".into()],
        };
        let (outcome, _) = run(&v, &repo, &base, &wt, "nexora/val/fail", &prog, &spec(), Duration::from_secs(60)).unwrap();
        assert_eq!(outcome, ValidationOutcome::Failed);
        git::remove_worktree(&repo, &wt).ok();
        std::fs::remove_dir_all(&repo).ok();
    }

    // ponytail: los canarios que dependen de ESCRITURA dentro del sandbox
    // (fuente modificada -> PolicyViolation, escritura fuera -> bloqueada) están
    // PENDIENTES: en esta máquina `codex sandbox` corre read-only y ningún token
    // `sandbox_permissions` (disk-write-cwd/disk-full-write-access/workspace-write)
    // concede escritura ("Acceso denegado"). Hasta resolver la config de
    // permisos de escritura (probablemente un --permissions-profile en
    // config.toml), esos canarios no se pueden ejercitar en vivo. La LÓGICA de
    // PolicyViolation por cambios inesperados/HEAD movido ya está cubierta de
    // forma determinista por `classify_covers_all_outcomes`.
}
