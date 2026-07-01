use super::AgentAdapter;
use crate::events::{parse_codex_line, AgentEvent};
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Codex;

impl AgentAdapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn name(&self) -> &'static str {
        "Codex CLI"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["backend", "architecture", "tests", "refactor"]
    }
    fn build_command(&self, _prompt: &str, dir: &Path, _safe: bool) -> Command {
        // _safe: el sandbox de Windows de Codex no arranca (helper ausente), así que
        // el modo seguro de Codex se refuerza vía prompt, no con flag de sandbox.
        // El prompt va por STDIN (no como argumento): `codex exec` sin PROMPT lee las
        // instrucciones de stdin. Así un prompt multilínea no se trunca al pasar por
        // `cmd /c` en Windows (cmd corta la línea en el primer salto de línea).
        let d = dir.to_string_lossy().into_owned();
        // -s danger-full-access: el sandbox de Windows (codex-windows-sandbox-setup.exe)
        // no arranca en muchas instalaciones ("program not found"), lo que bloquea TODA
        // ejecución de comandos y escritura de archivos en modo workspace-write/read-only.
        // danger-full-access es un modo -s OFICIAL que omite ese helper; el agente trabaja
        // directo en la carpeta del proyecto (igual que Claude con acceptEdits).
        #[cfg(windows)]
        let mut c = {
            let mut c = Command::new("cmd");
            c.args(["/c", "codex", "exec", "--json", "-s", "danger-full-access",
                "--skip-git-repo-check", "-C", d.as_str()]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let mut c = Command::new("codex");
            c.args(["exec", "--json", "-s", "danger-full-access", "--skip-git-repo-check", "-C", d.as_str()]);
            c
        };
        // stdin abierto: start_task escribe el prompt y CIERRA el pipe (EOF). Sin EOF
        // codex se colgaría esperando más entrada; con EOF lee el prompt y arranca.
        c.stdin(Stdio::piped());
        // CREATE_NO_WINDOW: evita que `cmd /c` abra una ventana de consola visible.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c
    }
    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        parse_codex_line(line)
    }
}
