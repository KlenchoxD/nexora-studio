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
    fn build_command(&self, _prompt: &str, dir: &Path, safe: bool) -> Command {
        // safe = política de permisos: "plan" planifica sin tocar archivos;
        // por defecto "acceptEdits" puede editar directo en la carpeta.
        let mode = if safe { "plan" } else { "acceptEdits" };
        // IMPORTANTE: el prompt va por STDIN, NO como argumento. En Windows el CLI es
        // un .cmd y se invoca vía `cmd /c`; un prompt MULTILÍNEA pasado como argumento
        // hace que cmd corte la línea de comando en el primer salto de línea y se
        // pierdan los flags que van después (--output-format), dejando a Claude en
        // salida de TEXTO PLANO (0 tokens, sin timeline). `claude -p` sin valor lee el
        // prompt de stdin; start_task lo escribe ahí. Aquí solo abrimos el pipe.
        #[cfg(windows)]
        let mut c = {
            let mut c = Command::new("cmd");
            c.args(["/c", "claude", "-p",
                "--output-format", "stream-json", "--verbose",
                "--permission-mode", mode]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let mut c = Command::new("claude");
            c.args(["-p", "--output-format", "stream-json",
                "--verbose", "--permission-mode", mode]);
            c
        };
        // Si hay ANTHROPIC_API_KEY en el entorno del sistema, Claude Code la usará
        // antes que la cuenta OAuth, produciendo "Invalid API key". La quitamos
        // del proceso hijo para forzar el login de cuenta personal (sin api).
        c.current_dir(dir).stdin(Stdio::piped()).env_remove("ANTHROPIC_API_KEY");
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
