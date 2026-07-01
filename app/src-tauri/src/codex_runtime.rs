//! Localizador del runtime de Codex (control A: reparar disponibilidad del sandbox).
//!
//! En Windows coexisten varias instalaciones de Codex (App, standalone, npm,
//! extensión). El `codex.exe` activo en PATH puede NO estar emparejado con su
//! directorio `codex-resources` (que contiene `codex-windows-sandbox-setup.exe`),
//! y entonces `codex sandbox` falla con "program not found" — aunque el helper SÍ
//! exista en otra release. Diagnóstico confirmado en esta máquina.
//!
//! Solución: NO usar el primer `codex` del PATH. Localizar una RELEASE STANDALONE
//! coherente (bin\codex.exe + codex-resources\helper de la MISMA versión) y
//! lanzar ese exe absoluto con un PATH que anteponga su `bin` y `codex-resources`.
//! Así el sandbox directo (determinista, sin modelo) queda disponible.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexRuntime {
    pub executable: PathBuf,     // <release>\bin\codex.exe
    pub resources_dir: PathBuf,  // <release>\codex-resources
    pub sandbox_helper: PathBuf, // <resources>\codex-windows-sandbox-setup.exe
    pub version: String,         // "0.142.3"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Clave de orden numérica para versiones "a.b.c" (fallback a 0 en no numéricos).
fn version_key(v: &str) -> Vec<u32> {
    v.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect()
}

/// Localiza TODOS los runtimes standalone coherentes (exe + helper presentes),
/// ordenados de más nuevo a más viejo.
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
        // Pareja COHERENTE: exe y helper de la misma release.
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

/// El runtime coherente más nuevo disponible, si hay alguno.
pub fn best() -> Option<CodexRuntime> {
    locate_all().into_iter().next()
}

impl CodexRuntime {
    /// PATH que antepone `bin` y `codex-resources` de ESTA release al PATH actual,
    /// para que el exe encuentre su helper emparejado y no otro desemparejado.
    fn controlled_path(&self) -> std::ffi::OsString {
        let bin = self
            .executable
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let orig = std::env::var_os("PATH").unwrap_or_default();
        let mut parts = vec![bin, self.resources_dir.clone()];
        parts.extend(std::env::split_paths(&orig));
        std::env::join_paths(parts).unwrap_or(orig)
    }

    /// `Command` para `codex sandbox` con el exe ABSOLUTO, PATH controlado y el
    /// modo pedido. El llamador agrega `--`, programa y argumentos.
    pub fn sandbox_command(&self, mode: SandboxMode) -> Command {
        let mut c = Command::new(&self.executable);
        c.env("PATH", self.controlled_path());
        if let SandboxMode::Unelevated = mode {
            c.args(["-c", "windows.sandbox=\"unelevated\""]);
        }
        c.arg("sandbox");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c
    }

    /// Ejecuta un comando dentro del sandbox de Codex. Devuelve el exit code REAL
    /// del proceso (sin modelo de por medio). Éste es el runner determinista.
    pub fn run_sandboxed(
        &self,
        mode: SandboxMode,
        program: &Path,
        args: &[&str],
        cwd: &Path,
    ) -> Result<std::process::Output, String> {
        let mut c = self.sandbox_command(mode);
        c.arg("--").arg(program);
        c.args(args);
        c.current_dir(cwd);
        c.output().map_err(|e| format!("codex sandbox falló: {e}"))
    }

    /// Prueba la disponibilidad del sandbox ejecutando un comando trivial.
    /// Prefiere elevado; si no, unelevated; si ninguno, Unavailable.
    pub fn probe(&self) -> SandboxCapability {
        if self.probe_mode(SandboxMode::Elevated) {
            SandboxCapability::Elevated
        } else if self.probe_mode(SandboxMode::Unelevated) {
            SandboxCapability::Unelevated
        } else {
            SandboxCapability::Unavailable
        }
    }

    fn probe_mode(&self, mode: SandboxMode) -> bool {
        let marker = "NEXORA_SANDBOX_OK";
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let cmd_exe = format!("{windir}\\System32\\cmd.exe");
        let mut c = self.sandbox_command(mode);
        c.args(["--", &cmd_exe, "/c", "echo", marker]);
        match c.output() {
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
        // Dependiente del entorno: si hay releases, cada runtime debe ser una
        // pareja REAL (exe + helper existen). Si no hay, la lista es vacía.
        for rt in locate_all() {
            assert!(rt.executable.is_file(), "exe debe existir: {:?}", rt.executable);
            assert!(rt.sandbox_helper.is_file(), "helper debe existir: {:?}", rt.sandbox_helper);
            assert!(!rt.version.is_empty());
        }
    }

    // Prueba VIVA (ignorada por defecto: lanza codex y depende del entorno).
    // Ejecutar con: cargo test live_sandbox_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_sandbox_probe() {
        let rt = best().expect("debe haber un runtime standalone coherente");
        eprintln!("runtime: {:?} v{}", rt.executable, rt.version);
        let cap = rt.probe();
        eprintln!("capability: {cap:?}");
        assert_ne!(cap, SandboxCapability::Unavailable, "el sandbox emparejado debe estar disponible");

        // ejecución determinista real dentro del sandbox
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let cmd_exe = std::path::PathBuf::from(format!("{windir}\\System32\\cmd.exe"));
        let out = rt
            .run_sandboxed(SandboxMode::Elevated, &cmd_exe, &["/c", "echo", "NEXORA_RUN_OK"], &std::env::temp_dir())
            .expect("run_sandboxed");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("NEXORA_RUN_OK"));
    }
}
