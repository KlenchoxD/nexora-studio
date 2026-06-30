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
    fn build_command(&self, prompt: &str, dir: &Path, _safe: bool) -> Command {
        // _safe: el sandbox de Windows de Codex no arranca (helper ausente), así que
        // el modo seguro de Codex se refuerza vía prompt, no con flag de sandbox.
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
                "--skip-git-repo-check", "-C", d.as_str(), prompt]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let mut c = Command::new("codex");
            c.args(["exec", "--json", "-s", "danger-full-access", "--skip-git-repo-check", "-C", d.as_str(), prompt]);
            c
        };
        c.stdin(Stdio::null()); // codex se cuelga esperando stdin
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
