use super::AgentAdapter;
use crate::events::{parse_claude_line, AgentEvent};
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Claude;

impl AgentAdapter for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn name(&self) -> &'static str {
        "Claude Code"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["frontend", "ui", "debugging", "docs", "review"]
    }
    fn build_command(&self, prompt: &str, dir: &Path) -> Command {
        // Windows: claude se instala como .cmd wrapper; necesita cmd /c para resolverse.
        #[cfg(windows)]
        let mut c = {
            let mut c = Command::new("cmd");
            c.args(["/c", "claude", "-p", prompt,
                "--output-format", "stream-json", "--verbose",
                "--permission-mode", "acceptEdits"]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let mut c = Command::new("claude");
            c.args(["-p", prompt, "--output-format", "stream-json",
                "--verbose", "--permission-mode", "acceptEdits"]);
            c
        };
        // Si hay ANTHROPIC_API_KEY en el entorno del sistema, Claude Code la usará
        // antes que la cuenta OAuth, produciendo "Invalid API key". La quitamos
        // del proceso hijo para forzar el login de cuenta personal (sin api).
        c.current_dir(dir).stdin(Stdio::null()).env_remove("ANTHROPIC_API_KEY");
        // CREATE_NO_WINDOW: evita que `cmd /c` abra una ventana de consola visible.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c
    }
    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        parse_claude_line(line)
    }
}
