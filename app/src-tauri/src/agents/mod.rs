//! Contrato del adaptador de agentes. El core habla con este trait, nunca con
//! una CLI directamente — punto de extensión para Gemini/Aider/etc. (Fase 4).

use crate::events::AgentEvent;
use std::path::Path;
use std::process::Command;

pub mod claude;
pub mod codex;

pub trait AgentAdapter {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> &'static [&'static str];

    /// Comando listo para lanzar: programa, args, cwd = carpeta del proyecto y
    /// **stdin cerrado** (ambas CLIs se cuelgan esperando stdin — ver
    /// CAPTURE-NOTES). El runner solo añade la captura de stdout.
    fn build_command(&self, prompt: &str, dir: &Path) -> Command;

    /// Normaliza una línea JSONL del agente a eventos comunes.
    fn parse_line(&self, line: &str) -> Vec<AgentEvent>;
}

#[cfg(test)]
mod tests {
    use super::claude::Claude;
    use super::codex::Codex;
    use super::AgentAdapter;
    use std::path::Path;

    fn args_of(c: &std::process::Command) -> Vec<String> {
        c.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    // En Windows las CLIs son shims .cmd y se invocan vía `cmd /c <cli> ...`,
    // así que el programa es "cmd" y la CLI real es el primer arg tras "/c".
    #[test]
    fn claude_launches_headless_streamjson() {
        let cmd = Claude.build_command("hola", Path::new("/tmp/wt"));
        let a = args_of(&cmd);
        let prog = cmd.get_program().to_string_lossy().into_owned();
        assert!(prog == "claude" || (prog == "cmd" && a.contains(&"claude".to_string())));
        assert!(a.contains(&"-p".to_string()));
        assert!(a.contains(&"stream-json".to_string()));
        assert!(a.contains(&"--output-format".to_string()));
    }

    #[test]
    fn codex_launches_exec_json_in_dir() {
        let cmd = Codex.build_command("hola", Path::new("/tmp/proj"));
        let a = args_of(&cmd);
        let prog = cmd.get_program().to_string_lossy().into_owned();
        assert!(prog == "codex" || (prog == "cmd" && a.contains(&"codex".to_string())));
        assert!(a.contains(&"exec".to_string()));
        assert!(a.contains(&"--json".to_string()));
        assert!(a.contains(&"-C".to_string()));
    }
}
