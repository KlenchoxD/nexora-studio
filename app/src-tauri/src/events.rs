//! Normalización del stream JSONL de los agentes a un modelo de eventos común.
//! Esquema de Claude confirmado con captura real (ver specs/.../samples).
//! El de Codex se cierra en T006; aquí es defensivo con fallback `Raw`.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Sesión iniciada. `api_key_source` permite verificar que NO se usa API key.
    Started { session_id: Option<String>, model: Option<String>, api_key_source: Option<String> },
    /// Mensaje/razonamiento del agente.
    Step { text: String },
    /// Herramienta/comando invocado por el agente. `detail` lleva el comando real
    /// (p.ej. "npm run build") cuando se conoce, para pintarlo como bloque de acción.
    ToolUse { name: String, detail: Option<String> },
    /// Cambio de archivo (crear/editar/borrar). Se pinta como tarjeta en el timeline.
    /// `op` (add/edit/delete) NO se llama `kind` para no chocar con el tag serde del enum.
    FileChange { path: String, op: Option<String> },
    /// Consumo de tokens (y coste si la CLI lo reporta).
    TokenUsage { input: u64, output: u64, cost_usd: Option<f64> },
    /// Tarea terminada.
    Done { success: bool, summary: Option<String>, cost_usd: Option<f64> },
    /// Error del agente.
    Error { message: String },
    /// Evento desconocido — se registra crudo, nunca se convierte en métrica.
    Raw { json: String },
}

impl AgentEvent {
    /// Etiqueta corta para la columna `kind` de `agent_event`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            AgentEvent::Started { .. } => "started",
            AgentEvent::Step { .. } => "step",
            AgentEvent::ToolUse { .. } => "tool_use",
            AgentEvent::FileChange { .. } => "file_change",
            AgentEvent::TokenUsage { .. } => "token_usage",
            AgentEvent::Done { .. } => "done",
            AgentEvent::Error { .. } => "error",
            AgentEvent::Raw { .. } => "raw",
        }
    }
}

fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

/// Parsea una línea de `claude -p --output-format stream-json`.
/// Una línea puede producir varios eventos (p.ej. texto + uso de tokens).
pub fn parse_claude_line(line: &str) -> Vec<AgentEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return vec![AgentEvent::Raw { json: line.to_string() }],
    };

    match v.get("type").and_then(|t| t.as_str()) {
        // Silenciar hooks internos de Claude (hook_started / hook_response)
        Some("system") if matches!(
            v.get("subtype").and_then(|s| s.as_str()),
            Some("hook_started") | Some("hook_response")
        ) => vec![],
        Some("system") if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            vec![AgentEvent::Started {
                session_id: str_field(&v, "session_id"),
                model: str_field(&v, "model"),
                api_key_source: str_field(&v, "apiKeySource"),
            }]
        }
        Some("assistant") => {
            let mut out = Vec::new();
            if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                for item in content {
                    match item.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = str_field(item, "text") {
                                out.push(AgentEvent::Step { text: t });
                            }
                        }
                        Some("tool_use") => {
                            if let Some(n) = str_field(item, "name") {
                                let input = item.get("input");
                                match n.as_str() {
                                    // Ediciones de archivo → tarjeta de archivo
                                    "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
                                        let path = input
                                            .and_then(|i| i.get("file_path").or_else(|| i.get("notebook_path")))
                                            .and_then(|x| x.as_str());
                                        if let Some(p) = path {
                                            let op = if n == "Write" { "add" } else { "edit" };
                                            out.push(AgentEvent::FileChange {
                                                path: p.to_string(),
                                                op: Some(op.to_string()),
                                            });
                                        } else {
                                            out.push(AgentEvent::ToolUse { name: n, detail: None });
                                        }
                                    }
                                    // Comando de shell → bloque con el comando real
                                    "Bash" => {
                                        let cmd = input.and_then(|i| i.get("command")).and_then(|x| x.as_str());
                                        out.push(AgentEvent::ToolUse {
                                            name: "command".into(),
                                            detail: cmd.map(|c| c.to_string()),
                                        });
                                    }
                                    _ => out.push(AgentEvent::ToolUse { name: n, detail: None }),
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(u) = v.pointer("/message/usage") {
                let input = u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                let output = u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                if input > 0 || output > 0 {
                    out.push(AgentEvent::TokenUsage { input, output, cost_usd: None });
                }
            }
            if out.is_empty() {
                out.push(AgentEvent::Raw { json: line.to_string() });
            }
            out
        }
        Some("result") => {
            let success = v.get("subtype").and_then(|s| s.as_str()) == Some("success");
            vec![AgentEvent::Done {
                success,
                summary: str_field(&v, "result"),
                cost_usd: v.get("total_cost_usd").and_then(|x| x.as_f64()),
            }]
        }
        // Silenciar eventos de infraestructura que no aportan info al usuario
        Some("rate_limit_event") | Some("user") => vec![],
        _ => vec![AgentEvent::Raw { json: line.to_string() }],
    }
}

/// Parsea una línea de `codex exec --json`.
/// Esquema real observado: thread.started, turn.started, message (role/content),
/// tool_call, tool_output, turn.completed, thread.completed.
pub fn parse_codex_line(line: &str) -> Vec<AgentEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return vec![AgentEvent::Raw { json: line.to_string() }],
    };
    match v.get("type").and_then(|t| t.as_str()) {
        // Sesión iniciada
        Some("thread.started") => {
            return vec![AgentEvent::Started { session_id: str_field(&v, "thread_id"), model: None, api_key_source: None }];
        }
        // Ignorar silenciosamente
        Some("turn.started") => return vec![],
        // item.started: comando → bloque con el comando real. file_change se emite en
        // completed (para no duplicar). agent_message/reasoning → el texto llega en completed.
        Some("item.started") => {
            let itype = v.pointer("/item/type").and_then(|x| x.as_str()).unwrap_or("");
            match itype {
                "agent_message" | "reasoning" | "file_change" => return vec![],
                "command_execution" => {
                    let cmd = v.pointer("/item/command").and_then(|x| x.as_str());
                    return vec![AgentEvent::ToolUse { name: "command".into(), detail: cmd.map(String::from) }];
                }
                _ => {
                    let name = v.pointer("/item/name").and_then(|x| x.as_str())
                        .unwrap_or(if itype.is_empty() { "command" } else { itype });
                    return vec![AgentEvent::ToolUse { name: name.to_string(), detail: None }];
                }
            }
        }
        // item.completed: agent_message → texto; file_change → tarjetas de archivo.
        Some("item.completed") => {
            let itype = v.pointer("/item/type").and_then(|x| x.as_str()).unwrap_or("");
            if itype == "agent_message" {
                if let Some(t) = v.pointer("/item/text").and_then(|x| x.as_str()) {
                    if !t.trim().is_empty() {
                        return vec![AgentEvent::Step { text: t.to_string() }];
                    }
                }
            }
            if itype == "file_change" {
                if let Some(changes) = v.pointer("/item/changes").and_then(|c| c.as_array()) {
                    let out: Vec<AgentEvent> = changes.iter().filter_map(|c| {
                        c.get("path").and_then(|x| x.as_str()).map(|p| AgentEvent::FileChange {
                            path: p.to_string(),
                            op: c.get("kind").and_then(|x| x.as_str()).map(String::from),
                        })
                    }).collect();
                    if !out.is_empty() { return out; }
                }
            }
            return vec![];
        }
        // Respuesta del agente (esquema alternativo top-level)
        Some("message") => {
            if v.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                let mut out = Vec::new();
                if let Some(content) = v.get("content").and_then(|c| c.as_array()) {
                    for item in content {
                        if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                            out.push(AgentEvent::Step { text: t.to_string() });
                        }
                    }
                }
                // fallback: campo text directo
                if out.is_empty() {
                    if let Some(t) = str_field(&v, "content")
                        .or_else(|| str_field(&v, "text"))
                    {
                        out.push(AgentEvent::Step { text: t });
                    }
                }
                if !out.is_empty() { return out; }
            }
        }
        // Herramienta
        Some("tool_call") => {
            let name = str_field(&v, "name")
                .or_else(|| v.pointer("/function/name").and_then(|x| x.as_str()).map(String::from))
                .unwrap_or_else(|| "tool".into());
            return vec![AgentEvent::ToolUse { name, detail: None }];
        }
        // Salida de herramienta — silencio (ruido sin valor para el usuario)
        Some("tool_output") => return vec![],
        // Turno completo: extraer uso de tokens
        Some("turn.completed") => {
            if let Some(u) = v.get("usage") {
                let input = u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                let output = u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                if input > 0 || output > 0 {
                    return vec![AgentEvent::TokenUsage { input, output, cost_usd: None }];
                }
            }
            return vec![];
        }
        // Hilo completo — tarea terminada
        Some("thread.completed") => {
            return vec![AgentEvent::Done {
                success: true,
                summary: str_field(&v, "summary").or_else(|| str_field(&v, "result")),
                cost_usd: v.get("total_cost_usd").and_then(|x| x.as_f64()),
            }];
        }
        _ => {}
    }
    // Fallback: campo text en cualquier nivel
    if let Some(t) = str_field(&v, "text")
        .or_else(|| v.pointer("/item/text").and_then(|x| x.as_str()).map(String::from))
    {
        return vec![AgentEvent::Step { text: t }];
    }
    vec![AgentEvent::Raw { json: line.to_string() }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_init_exposes_api_key_source() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc","model":"claude-opus-4-8","apiKeySource":"none"}"#;
        assert_eq!(
            parse_claude_line(line),
            vec![AgentEvent::Started {
                session_id: Some("abc".into()),
                model: Some("claude-opus-4-8".into()),
                api_key_source: Some("none".into()),
            }]
        );
    }

    #[test]
    fn claude_assistant_text_and_usage() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hola"}],"usage":{"input_tokens":10,"output_tokens":4}}}"#;
        let ev = parse_claude_line(line);
        assert_eq!(ev[0], AgentEvent::Step { text: "hola".into() });
        assert_eq!(ev[1], AgentEvent::TokenUsage { input: 10, output: 4, cost_usd: None });
    }

    #[test]
    fn claude_tool_use() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"apply_patch","input":{}}]}}"#;
        assert_eq!(parse_claude_line(line), vec![AgentEvent::ToolUse { name: "apply_patch".into(), detail: None }]);
    }

    #[test]
    fn claude_edit_is_file_change() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"src/app.tsx"}}]}}"#;
        assert_eq!(parse_claude_line(line), vec![AgentEvent::FileChange { path: "src/app.tsx".into(), op: Some("edit".into()) }]);
    }

    #[test]
    fn claude_bash_is_command_with_detail() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"npm run build"}}]}}"#;
        assert_eq!(parse_claude_line(line), vec![AgentEvent::ToolUse { name: "command".into(), detail: Some("npm run build".into()) }]);
    }

    #[test]
    fn codex_file_change_completed_emits_card() {
        let line = r#"{"type":"item.completed","item":{"type":"file_change","changes":[{"path":"C:/x/hola.txt","kind":"add"}]}}"#;
        assert_eq!(parse_codex_line(line), vec![AgentEvent::FileChange { path: "C:/x/hola.txt".into(), op: Some("add".into()) }]);
    }

    #[test]
    fn codex_command_started_carries_command() {
        let line = r#"{"type":"item.started","item":{"type":"command_execution","command":"git status"}}"#;
        assert_eq!(parse_codex_line(line), vec![AgentEvent::ToolUse { name: "command".into(), detail: Some("git status".into()) }]);
    }

    #[test]
    fn claude_result_done() {
        let line = r#"{"type":"result","subtype":"success","result":"hola","total_cost_usd":0.08}"#;
        assert_eq!(
            parse_claude_line(line),
            vec![AgentEvent::Done { success: true, summary: Some("hola".into()), cost_usd: Some(0.08) }]
        );
    }

    #[test]
    fn codex_agent_message_in_item_completed_is_step() {
        // El texto real del agente llega como item.completed/agent_message.
        let line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Hola"}}"#;
        assert_eq!(parse_codex_line(line), vec![AgentEvent::Step { text: "Hola".into() }]);
    }

    #[test]
    fn codex_command_execution_is_silent_in_completed() {
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","exit_code":0,"status":"completed"}}"#;
        assert!(parse_codex_line(line).is_empty());
    }

    #[test]
    fn unknown_and_garbage_fall_back_to_raw() {
        // rate_limit_event ahora se silencia (infraestructura, sin valor para el usuario)
        let rate = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#;
        assert!(parse_claude_line(rate).is_empty());
        // un type realmente desconocido sí cae en Raw
        let weird = r#"{"type":"some_future_event","x":1}"#;
        assert!(matches!(parse_claude_line(weird).as_slice(), [AgentEvent::Raw { .. }]));
        assert!(matches!(parse_claude_line("not json").as_slice(), [AgentEvent::Raw { .. }]));
        assert!(parse_claude_line("   ").is_empty());
    }
}
