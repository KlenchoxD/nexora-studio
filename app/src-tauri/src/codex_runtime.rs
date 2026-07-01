//! Localizador y frontera de seguridad del runtime de Codex (control A).
//!
//! En Windows coexisten varias instalaciones de Codex (App, standalone, npm,
//! extensión). El `codex.exe` activo en PATH puede NO estar emparejado con su
//! directorio `codex-resources` (que contiene `codex-windows-sandbox-setup.exe`),
//! y entonces `codex sandbox` falla con "program not found" aunque el helper SÍ
//! exista en otra release. Diagnóstico confirmado en esta máquina.
//!
//! Solución: NO usar el primer `codex` del PATH. Localizar una RELEASE STANDALONE
//! coherente (bin\codex.exe + codex-resources\helper de la MISMA versión,
//! verificada ejecutando el exe), lanzar ese exe absoluto con un PATH filtrado
//! que anteponga su `bin` y `codex-resources`, con entorno MÍNIMO (sin secretos).

use crate::trusted_exec;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexRuntime {
    pub executable: PathBuf,     // <release>\bin\codex.exe (canónico)
    pub resources_dir: PathBuf,  // <release>\codex-resources (canónico)
    pub sandbox_helper: PathBuf, // <resources>\codex-windows-sandbox-setup.exe (canónico)
    pub version: String,         // "0.142.3" (del nombre de release)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum SandboxMode {
    /// Preferido: sandbox elevado nativo de Windows.
    Elevated,
    /// Fallback: token restringido (no requiere elevación).
    Unelevated,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SandboxCapability {
    Elevated,
    Unelevated,
    Unavailable,
}

/// Runtime ya PROBADO: incluye el modo operativo, para que el ValidationRunner
/// no reejecute probes ni adivine el modo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidatedCodexRuntime {
    pub runtime: CodexRuntime,
    pub mode: SandboxMode,
}

fn releases_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".codex")
            .join("packages")
            .join("standalone")
            .join("releases"),
    )
}

/// "0.142.3-x86_64-pc-windows-msvc" -> "0.142.3".
fn parse_version(dir_name: &str) -> String {
    dir_name.split('-').next().unwrap_or(dir_name).to_string()
}

fn version_key(v: &str) -> Vec<u32> {
    v.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect()
}

/// Localiza TODOS los runtimes standalone coherentes (exe + helper presentes y
/// canonicalizables), ordenados de más nuevo a más viejo. No prueba el sandbox
/// (eso lo hace `discover`).
pub fn locate_all() -> Vec<CodexRuntime> {
    let mut out = Vec::new();
    let Some(dir) = releases_dir() else { return out };
    let Ok(entries) = std::fs::read_dir(&dir) else { return out };
    for e in entries.flatten() {
        let rel = e.path();
        if !rel.is_dir() {
            continue;
        }
        let exe = rel.join("bin").join("codex.exe");
        let res = rel.join("codex-resources");
        let helper = res.join("codex-windows-sandbox-setup.exe");
        // Pareja COHERENTE: exe y helper existen y se pueden canonicalizar
        // (fail-closed: si no se canonicaliza, se descarta).
        let (Ok(exe), Ok(res), Ok(helper)) =
            (exe.canonicalize(), res.canonicalize(), helper.canonicalize())
        else {
            continue;
        };
        if exe.is_file() && helper.is_file() {
            out.push(CodexRuntime {
                executable: exe,
                resources_dir: res,
                sandbox_helper: helper,
                version: parse_version(&e.file_name().to_string_lossy()),
            });
        }
    }
    out.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    out
}

/// Selector definitivo: prueba cada release coherente de más nueva a más vieja,
/// verifica que la versión del EJECUTABLE coincida, y devuelve la primera con
/// sandbox operativo (Elevated preferido, luego Unelevated). La release más
/// nueva puede tener el helper pero un sandbox roto: por eso se prueba, no se
/// asume.
pub fn discover() -> Option<ValidatedCodexRuntime> {
    for rt in locate_all() {
        if !rt.version_matches_executable() {
            continue;
        }
        match rt.probe() {
            SandboxCapability::Elevated => {
                return Some(ValidatedCodexRuntime { runtime: rt, mode: SandboxMode::Elevated })
            }
            SandboxCapability::Unelevated => {
                return Some(ValidatedCodexRuntime { runtime: rt, mode: SandboxMode::Unelevated })
            }
            SandboxCapability::Unavailable => continue,
        }
    }
    None
}

/// Nombre del permission profile de validación (definido en el CODEX_HOME
/// administrado por Nexora).
pub const VALIDATION_PROFILE: &str = "nexora-validation";

/// CODEX_HOME administrado por Nexora (separado de la config personal), con el
/// perfil `nexora-validation` que extiende `:workspace`: escritura confinada al
/// worktree (+temp), bloqueando archivos reales del usuario y `.git`. Verificado
/// empíricamente. Se crea/actualiza el config.toml de forma idempotente.
pub fn managed_codex_home() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or("no se encontró LOCALAPPDATA")?;
    let home = PathBuf::from(base).join("NexoraStudio").join("codex-validation");
    std::fs::create_dir_all(&home).map_err(|e| format!("crear codex-home: {e}"))?;
    // ponytail: el bloqueo de red por config no frenó ICMP en las pruebas; se
    // conserva `network.enabled=false` (puede limitar sockets) y se revisará si
    // hace falta aislamiento de red más fuerte.
    // StrictElevated (verificado empíricamente): el backend `elevated` SÍ aplica
    // reglas divididas (denegar :tmpdir/:slash_tmp cierra el escape a %TEMP%
    // global) y bloquea TCP/HTTPS. El UAC es de SETUP ÚNICO: con este CODEX_HOME
    // persistente, tras aprobarlo una vez, las validaciones (secuenciales y
    // concurrentes) NO vuelven a pedir UAC. unelevated NO puede denegar el temp
    // ("requires elevated backend") y tiene red más débil, por eso no es el modo
    // estricto. `.git`/`.codex` ya los protege `:workspace`.
    let cfg = "[windows]\n\
        sandbox = \"elevated\"\n\n\
        [permissions.nexora-validation]\n\
        description = \"Validacion aislada en un worktree desechable\"\n\
        extends = \":workspace\"\n\n\
        [permissions.nexora-validation.filesystem]\n\
        \":tmpdir\" = \"deny\"\n\
        \":slash_tmp\" = \"deny\"\n\n\
        [permissions.nexora-validation.network]\n\
        enabled = false\n";
    let cfg_path = home.join("config.toml");
    // escribe solo si cambió (idempotente)
    let need = std::fs::read_to_string(&cfg_path).map(|c| c != cfg).unwrap_or(true);
    if need {
        std::fs::write(&cfg_path, cfg).map_err(|e| format!("escribir config.toml: {e}"))?;
    }
    Ok(home)
}

impl CodexRuntime {
    /// Verifica que `bin\codex.exe` corresponda REALMENTE a la versión de la
    /// carpeta ejecutando `--version` (no basta el nombre del directorio).
    pub fn version_matches_executable(&self) -> bool {
        match Command::new(&self.executable).arg("--version").output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&self.version),
            Err(_) => false,
        }
    }

    /// PATH filtrado: antepone `bin` + `codex-resources` de ESTA release, y del
    /// PATH heredado descarta entradas relativas, vacías, no canonicalizables o
    /// que caigan dentro de `deny_roots` (repo/worktrees). Fail-closed: un fallo
    /// de `join_paths` es error, NO se restaura el PATH inseguro.
    fn filtered_path(&self, deny_roots: &[&Path]) -> Result<OsString, String> {
        let bin = self
            .executable
            .parent()
            .ok_or("el exe de codex no tiene directorio padre")?
            .to_path_buf();
        let mut parts = vec![bin, self.resources_dir.clone()];
        if let Some(orig) = std::env::var_os("PATH") {
            for d in std::env::split_paths(&orig) {
                if d.as_os_str().is_empty() || !d.is_absolute() {
                    continue;
                }
                let Ok(cd) = d.canonicalize() else { continue };
                // fail-closed: si no se puede comprobar contención, se excluye.
                let inside_denied = deny_roots
                    .iter()
                    .any(|r| trusted_exec::is_inside(&cd, r).unwrap_or(true));
                if inside_denied {
                    continue;
                }
                parts.push(cd);
            }
        }
        std::env::join_paths(parts).map_err(|e| format!("join_paths falló: {e}"))
    }

    /// `Command` para `codex sandbox` con exe absoluto, PATH filtrado, entorno
    /// MÍNIMO (sin secretos: `env_clear` + allowlist) y TEMP/TMP redirigidos a un
    /// directorio controlado. El llamador agrega `--`, programa y argumentos.
    fn sandbox_command(
        &self,
        mode: SandboxMode,
        cwd: &Path,
        temp_dir: &Path,
        deny_roots: &[&Path],
    ) -> Result<Command, String> {
        std::fs::create_dir_all(temp_dir)
            .map_err(|e| format!("no se pudo crear temp {temp_dir:?}: {e}"))?;
        let path = self.filtered_path(deny_roots)?;

        let mut c = Command::new(&self.executable);
        // Entorno explícito: elimina API keys, tokens y credenciales heredadas.
        c.env_clear();
        c.env("PATH", path);
        // Allowlist mínima NO secreta que Windows/Codex necesitan para arrancar.
        for k in [
            "SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "SystemDrive",
            "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA",
            "CODEX_HOME", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "OS",
        ] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        c.env("TEMP", temp_dir);
        c.env("TMP", temp_dir);
        c.current_dir(cwd);
        if let SandboxMode::Unelevated = mode {
            c.args(["-c", "windows.sandbox=\"unelevated\""]);
        }
        c.arg("sandbox");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        Ok(c)
    }

    /// `Command` completo (`codex sandbox -- program args`) listo para spawn. El
    /// ValidationRunner lo usa para aplicar timeout con control del proceso.
    pub fn command(
        &self,
        mode: SandboxMode,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        temp_dir: &Path,
        deny_roots: &[&Path],
    ) -> Result<Command, String> {
        let mut c = self.sandbox_command(mode, cwd, temp_dir, deny_roots)?;
        c.arg("--").arg(program);
        c.args(args);
        Ok(c)
    }

    /// `Command` de VALIDACIÓN: `codex sandbox -P nexora-validation -C cwd --
    /// program args`, con CODEX_HOME administrado (perfil que confina escritura
    /// al worktree). El perfil es el mecanismo correcto de escritura controlada
    /// en 0.142.3 (NO `sandbox_permissions`). Entorno mínimo + PATH filtrado.
    pub fn validation_command(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        deny_roots: &[&Path],
    ) -> Result<Command, String> {
        let codex_home = managed_codex_home()?;
        let path = self.filtered_path(deny_roots)?;

        let mut c = Command::new(&self.executable);
        c.env_clear();
        c.env("PATH", path);
        c.env("CODEX_HOME", &codex_home); // config personal NO se usa
        // Allowlist NO secreta. Incluye TEMP/TMP del sistema: el helper de setup
        // del sandbox los necesita para crear sus binarios (redirigirlos al
        // worktree rompe el setup: orchestrator_helper_incomplete). El perfil
        // `:workspace` ya permite escritura en temp, así que el temp de los tests
        // queda igualmente contenido/ephemeral.
        for k in [
            "SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "SystemDrive",
            "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA",
            "TEMP", "TMP", "USERNAME", "USERDOMAIN", "SESSIONNAME",
            "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "OS",
        ] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        c.arg("sandbox");
        c.args(["-P", VALIDATION_PROFILE]);
        c.arg("-C").arg(cwd);
        c.arg("--").arg(program);
        c.args(args);
        c.current_dir(cwd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        Ok(c)
    }

    /// Ejecuta `program args` dentro del sandbox de Codex (exit code REAL, sin
    /// modelo). `deny_roots` son el repo y sus worktrees (para filtrar el PATH).
    pub fn run_sandboxed(
        &self,
        mode: SandboxMode,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        temp_dir: &Path,
        deny_roots: &[&Path],
    ) -> Result<Output, String> {
        self.command(mode, program, args, cwd, temp_dir, deny_roots)?
            .output()
            .map_err(|e| format!("codex sandbox falló: {e}"))
    }

    /// Prueba disponibilidad ejecutando un comando trivial. Prefiere UNELEVATED:
    /// no dispara UAC (elevated usa ShellExecuteExW con elevación → prompt de UAC
    /// y falla en no-interactivo/paralelo con 1223). La validación usa unelevated.
    pub fn probe(&self) -> SandboxCapability {
        if self.probe_mode(SandboxMode::Unelevated) {
            SandboxCapability::Unelevated
        } else if self.probe_mode(SandboxMode::Elevated) {
            SandboxCapability::Elevated
        } else {
            SandboxCapability::Unavailable
        }
    }

    fn probe_mode(&self, mode: SandboxMode) -> bool {
        let marker = "NEXORA_SANDBOX_OK";
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let cmd_exe = PathBuf::from(format!("{windir}\\System32\\cmd.exe"));
        match self.run_sandboxed(
            mode,
            &cmd_exe,
            &["/c", "echo", marker],
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &[],
        ) {
            Ok(o) => o.status.success() && String::from_utf8_lossy(&o.stdout).contains(marker),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_from_release_dir() {
        assert_eq!(parse_version("0.142.3-x86_64-pc-windows-msvc"), "0.142.3");
        assert_eq!(parse_version("1.0.0"), "1.0.0");
    }

    #[test]
    fn version_key_orders_numerically() {
        assert!(version_key("0.142.3") > version_key("0.99.9"));
        assert!(version_key("1.0.0") > version_key("0.142.3"));
    }

    #[test]
    fn locate_returns_coherent_pairs_when_present() {
        for rt in locate_all() {
            assert!(rt.executable.is_file(), "exe debe existir: {:?}", rt.executable);
            assert!(rt.sandbox_helper.is_file(), "helper debe existir: {:?}", rt.sandbox_helper);
            assert!(rt.executable.is_absolute() && rt.resources_dir.is_absolute());
            assert!(!rt.version.is_empty());
        }
    }

    // Prueba VIVA (ignorada por defecto). Verifica que, CON entorno mínimo
    // (env_clear + allowlist) y PATH filtrado, el sandbox sigue operativo.
    // Ejecutar: cargo test live_discover -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_discover() {
        let v = discover().expect("debe descubrir un runtime con sandbox operativo");
        eprintln!("runtime: {:?} v{} mode={:?}", v.runtime.executable, v.runtime.version, v.mode);

        // ejecución determinista real con entorno mínimo
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let cmd_exe = PathBuf::from(format!("{windir}\\System32\\cmd.exe"));
        let tmp = std::env::temp_dir().join("nexora_val_tmp");
        let out = v
            .runtime
            .run_sandboxed(v.mode, &cmd_exe, &["/c", "echo", "NEXORA_RUN_OK"], &std::env::temp_dir(), &tmp, &[])
            .expect("run_sandboxed");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("NEXORA_RUN_OK"));
    }
}
