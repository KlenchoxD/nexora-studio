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
    /// No se pudo inspeccionar el estado git tras la ejecución (error de
    /// infraestructura): fail-closed, NUNCA se aprueba sin verificar.
    Unverified,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationEvidence {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub head_changed: bool,
    pub changed: Vec<ChangedFile>,
    pub timed_out: bool,
    /// Modo de seguridad efectivo y garantía por dimensión: Nexora NUNCA afirma
    /// más aislamiento del verificado.
    pub security_mode: ValidationSecurityMode,
    pub assurance: ValidationAssurance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ValidationSecurityMode {
    /// Backend elevado con perfil que deniega temp global. UAC de setup único.
    StrictElevated,
    /// Fallback sin UAC; NO deniega temp global ni aísla red del todo. Solo para
    /// repos que el usuario marque como confiables.
    CompatibleUnelevated,
    /// Backend Linux vía WSL2.
    Wsl2,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AssuranceState {
    Verified,
    Partial,
    Unverified,
    Unavailable,
}

/// Garantía de aislamiento por dimensión (honesta, verificada empíricamente).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationAssurance {
    pub workspace_write_boundary: AssuranceState,
    pub git_write_protection: AssuranceState,
    pub environment_secrets_removed: AssuranceState,
    pub global_temp_isolation: AssuranceState,
    pub network_isolation: AssuranceState,
}

impl ValidationAssurance {
    /// StrictElevated (verificado): worktree/git/temp/env aislados; red PARCIAL
    /// (TCP/HTTPS bloqueado, ICMP residual).
    pub fn strict_elevated() -> Self {
        use AssuranceState::*;
        Self {
            workspace_write_boundary: Verified,
            git_write_protection: Verified,
            environment_secrets_removed: Verified,
            global_temp_isolation: Verified,
            network_isolation: Partial,
        }
    }

    /// CompatibleUnelevated: temp global escribible (no se puede denegar sin
    /// elevated) y red no verificada. NO usar con código no confiable.
    pub fn compatible_unelevated() -> Self {
        use AssuranceState::*;
        Self {
            workspace_write_boundary: Verified,
            git_write_protection: Verified,
            environment_secrets_removed: Verified,
            global_temp_isolation: Unavailable,
            network_isolation: Unverified,
        }
    }
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

    let args: Vec<&str> = program.args.iter().map(|s| s.as_str()).collect();
    let mut child = validated
        .runtime
        .validation_command(&program.executable, &args, wt_path, &deny)?
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar la validación: {e}"))?;

    // Drenar stdout/stderr CONCURRENTEMENTE en hilos: si esperáramos al proceso
    // con las tuberías llenas sin leer, se produciría un deadlock.
    use std::io::Read;
    let drain = |s: Option<std::process::ChildStdout>| {
        s.map(|mut s| std::thread::spawn(move || {
            let mut b = String::new();
            let _ = s.read_to_string(&mut b);
            b
        }))
    };
    let out_h = drain(child.stdout.take());
    let err_h = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut b = String::new();
            let _ = s.read_to_string(&mut b);
            b
        })
    });

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
    let stdout = out_h.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = err_h.and_then(|h| h.join().ok()).unwrap_or_default();

    // Verificación determinista del estado git tras la ejecución. FAIL-CLOSED:
    // si no se puede leer HEAD o el status, NO se aprueba (Unverified).
    let git_state = git::head_commit(wt_path).and_then(|h| git::changed_files(wt_path).map(|c| (h, c)));
    let (outcome, head_changed, changed) = match git_state {
        Ok((head_after, changed)) => {
            let head_changed = head_after != candidate_commit;
            let unexpected = spec.has_unexpected_changes(&changed);
            (classify(timed_out, exit_code, head_changed, unexpected), head_changed, changed)
        }
        Err(_) => (ValidationOutcome::Unverified, false, Vec::new()),
    };

    // Conservar el worktree como evidencia si NO aprobó; limpiar si Passed.
    if outcome == ValidationOutcome::Passed {
        let _ = git::remove_worktree(repo_root, wt_path);
    }

    Ok((
        outcome,
        ValidationEvidence {
            exit_code,
            stdout,
            stderr,
            head_changed,
            changed,
            timed_out,
            // El CODEX_HOME administrado usa el perfil elevado; la garantía se
            // reporta honestamente (red PARCIAL por ICMP residual).
            security_mode: ValidationSecurityMode::StrictElevated,
            assurance: ValidationAssurance::strict_elevated(),
        },
    ))
}

/// Mata el árbol de procesos por PID con `taskkill.exe` ABSOLUTO (no relativo,
/// para no arriesgar suplantación desde el cwd). `/T /F` termina los hijos.
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let taskkill = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()))
            .join("System32")
            .join("taskkill.exe");
        let _ = std::process::Command::new(taskkill)
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("/bin/kill")
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
        eprintln!("outcome={outcome:?} exit={:?}\nSTDOUT={}\nSTDERR={}", ev.exit_code, ev.stdout, ev.stderr);
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

    fn cmd_exe() -> PathBuf {
        PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()))
            .join("System32")
            .join("cmd.exe")
    }

    // Escritura DENTRO del worktree de un archivo NO permitido (app.js.bak fuera
    // de coverage/**) con exit 0 -> PolicyViolation. Prueba que el sandbox SÍ
    // deja escribir el workspace (perfil nexora-validation) y que el runner
    // detecta el cambio inesperado.
    #[test]
    #[ignore]
    fn canary_source_modified_is_policy_violation() {
        let Some((v, repo, base)) = setup() else { return };
        let wt = std::env::temp_dir().join(format!("nexora_vwt_{}", uuid::Uuid::new_v4()));
        let prog = SandboxedProgram {
            executable: cmd_exe(),
            args: vec!["/c".into(), "copy".into(), "/y".into(), "app.js".into(), "app.js.bak".into()],
        };
        let (outcome, ev) = run(&v, &repo, &base, &wt, "nexora/val/src", &prog, &spec(), Duration::from_secs(60)).unwrap();
        eprintln!("outcome={outcome:?} exit={:?} changed={:?} stderr={}", ev.exit_code, ev.changed, ev.stderr);
        assert_eq!(outcome, ValidationOutcome::PolicyViolation);
        git::remove_worktree(&repo, &wt).ok();
        std::fs::remove_dir_all(&repo).ok();
    }

    // Intento de escribir FUERA del worktree (archivo real). El sandbox lo
    // bloquea: el archivo externo NO se crea. (El comando falla -> Failed, pero
    // lo esencial es la contención: nada se escribió fuera.)
    #[test]
    #[ignore]
    fn canary_escape_write_is_contained() {
        let Some((v, repo, base)) = setup() else { return };
        let wt = std::env::temp_dir().join(format!("nexora_vwt_{}", uuid::Uuid::new_v4()));
        let escape = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("Documents")
            .join(format!("NEXORA_ESCAPE_{}.txt", uuid::Uuid::new_v4()));
        let prog = SandboxedProgram {
            executable: cmd_exe(),
            args: vec!["/c".into(), "copy".into(), "app.js".into(), escape.to_string_lossy().into()],
        };
        let _ = run(&v, &repo, &base, &wt, "nexora/val/escape", &prog, &spec(), Duration::from_secs(60)).unwrap();
        assert!(!escape.exists(), "el sandbox permitió escribir FUERA del worktree: {escape:?}");
        git::remove_worktree(&repo, &wt).ok();
        std::fs::remove_dir_all(&repo).ok();
    }
}
