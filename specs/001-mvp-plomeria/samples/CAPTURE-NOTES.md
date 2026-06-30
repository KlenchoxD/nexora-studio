# Notas de captura (T006 / T007)

Captura real ejecutada el 2026-06-29 contra las cuentas personales del usuario.

## ⚠️ Hallazgo crítico para el runner: ambas CLIs bloquean en stdin

Tanto `claude -p` como `codex exec` **leen de stdin y se quedan esperando** si
stdin no recibe EOF, aunque el prompt se pase como argumento:
- Claude: avisa `Warning: no stdin data received in 3s, proceeding without it`
  (pierde ~3s por tarea).
- Codex: se queda **colgado indefinidamente** en `Reading additional input from
  stdin...` (no continúa nunca).

**Regla para `runner.rs` (T011):** al lanzar cualquier agente, cerrar/redirigir
stdin (equivalente a `< /dev/null` / `Stdio::null()`). Sin esto, las tareas se
cuelgan. Aplica a TODOS los adaptadores.

## Claude — `claude -p --output-format stream-json --verbose`

Capturado en `claude-stream.jsonl`. Emite JSONL, un objeto por línea:

| `type` | subtype / contenido | → AgentEvent |
|---|---|---|
| `system` | `hook_started` / `hook_response` | ignorar/`Raw` (son hooks del entorno del worktree) |
| `system` | `init` — incluye `cwd`, `session_id`, `model`, `permissionMode`, **`apiKeySource`** | `Started` |
| `assistant` | `message.content[]` con `{type:text}` o `{type:tool_use}` + `message.usage` | `Step` / `ToolUse` + `TokenUsage` |
| `rate_limit_event` | `rate_limit_info.status` (allowed/…), `resetsAt` | `Raw` (o mostrar aviso de límite) |
| `result` | `result`, `total_cost_usd`, `usage`, `num_turns`, `duration_ms`, `permission_denials` | `Done` + coste final |

**Para SC-005 (sin API key):** el evento `init` trae `"apiKeySource":"none"`
cuando se usa login de suscripción. Nexora puede leer ese campo para **verificar
que NO se usa API key**. 

**Coste observado:** "hola" costó ~$0.08 por el contexto/caché de ESTA sesión
(hooks pesados). Un spawn limpio de Nexora será mucho más barato; el coste real
sale de `total_cost_usd` en `result`.

## Auth detection (T007)

- **Codex:** `codex login status` → `Logged in using ChatGPT` (exit 0).
  Confirma login de suscripción, sin API key. Comando ideal para `detect.rs`.
- **Claude:** no hay `whoami` limpio. Dos señales válidas sin tocar credenciales:
  1. el campo `apiKeySource` del evento `init` de un `-p` de prueba;
  2. el éxito/fallo de ese mismo `-p` (si la sesión expiró, falla con error real).

## Codex — `codex exec --json` (PENDIENTE)

No se logró sample limpio en local: con sandbox `read-only` + stdin abierto se
colgó (ver `codex-exec.jsonl`, que contiene solo la evidencia del bloqueo).
Resolver en implementación (T006) lanzando en un **worktree git real** con
`-s workspace-write`, `-C <worktree>` y **stdin cerrado**. Se sabe por la doc que
`--json` emite eventos JSONL; el parser se escribe defensivo (campos opcionales,
fallback `Raw`) y se ajusta con el sample real entonces.
