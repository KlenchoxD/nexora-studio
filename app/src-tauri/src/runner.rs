//! Lanza un agente y transmite sus eventos. Bloqueante por diseño: el llamador
//! lo corre en un hilo (Tauri puede emitir eventos desde cualquier hilo), así
//! evitamos tokio en el MVP (ponytail: async cuando la concurrencia lo exija).

use crate::agents::AgentAdapter;
use crate::events::AgentEvent;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;

/// Lee líneas de un stream, las normaliza y entrega cada evento. Aislado del
/// spawn del SO para poder testear el bucle sin lanzar un agente real.
pub fn pump<R: BufRead>(
    reader: R,
    parse: impl Fn(&str) -> Vec<AgentEvent>,
    mut on_event: impl FnMut(AgentEvent),
) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        for ev in parse(&line) {
            on_event(ev);
        }
    }
    Ok(())
}

/// Lanza el agente en `worktree`, transmite sus eventos y devuelve el exit code.
pub fn run<A: AgentAdapter>(
    adapter: &A,
    prompt: &str,
    worktree: &Path,
    on_event: impl FnMut(AgentEvent),
) -> Result<i32, String> {
    let mut cmd = adapter.build_command(prompt, worktree, false);
    cmd.stdout(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn falló: {e}"))?;
    let stdout = child.stdout.take().ok_or("sin stdout")?;
    pump(BufReader::new(stdout), |l| adapter.parse_line(l), on_event)
        .map_err(|e| format!("error leyendo stdout: {e}"))?;
    let status = child.wait().map_err(|e| format!("wait falló: {e}"))?;
    Ok(status.code().unwrap_or(-1))
    // ponytail: cancelación (kill del Child) se añade en T023 cuando exista la
    // cola de tareas; hoy el runner es de un solo disparo.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::parse_claude_line;
    use std::io::Cursor;

    #[test]
    fn pump_dispatches_events_per_line() {
        let stream = concat!(
            r#"{"type":"system","subtype":"init","apiKeySource":"none"}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hola"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","result":"hola","total_cost_usd":0.01}"#,
            "\n",
        );
        let mut got = Vec::new();
        pump(Cursor::new(stream), parse_claude_line, |e| got.push(e)).unwrap();

        assert!(matches!(got[0], AgentEvent::Started { .. }));
        assert!(got.iter().any(|e| matches!(e, AgentEvent::Step { text } if text == "hola")));
        assert!(matches!(got.last().unwrap(), AgentEvent::Done { success: true, .. }));
    }
}
