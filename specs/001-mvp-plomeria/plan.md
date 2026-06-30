# Implementation Plan: MVP Fase 0 — Plomería de orquestación paralela

**Branch**: `001-mvp-plomeria` | **Date**: 2026-06-29 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-mvp-plomeria/spec.md`

## Summary

Construir el núcleo de Nexora Studio: una app de escritorio Tauri que lanza
Codex CLI y Claude Code en modo headless, cada uno en su propio git worktree,
en paralelo, mostrando su actividad en vivo a partir de eventos JSON
estructurados, y permitiendo al usuario (jefe) revisar el diff de cada worktree
e integrarlo por merge. Sin API, sin credenciales: las CLIs heredan la sesión
personal ya iniciada por el usuario.

El producto entero descansa sobre una verdad técnica confirmada en la máquina:
ambas CLIs tienen modo no interactivo con salida JSONL
(`claude -p --output-format stream-json`, `codex exec --json`) y aceptan un
directorio de trabajo arbitrario, así que podemos apuntarlas a un worktree y
leer su stream sin parsear la TUI.

## Technical Context

**Language/Version**: Rust (estable, vía rustup) para el core Tauri;
TypeScript 5 + React 18 para el frontend.

**Primary Dependencies**:
- Tauri v2 (shell + IPC + spawn de procesos)
- tokio (procesos async, lectura de stdout línea a línea)
- serde / serde_json (parseo de eventos JSONL)
- rusqlite (SQLite embebido, bundled — sin dependencia externa de sistema)
- React + Vite; librería de UI a decidir en la fase de diseño (probable
  Tailwind + shadcn/ui). Monaco y xterm.js se difieren.

**Storage**: SQLite local (rusqlite), un archivo por instalación. Guarda
proyectos, tareas, eventos e historial.

**Testing**: `cargo test` para parser de eventos y gestor de worktrees
(las dos piezas de lógica no trivial). Frontend sin framework de test en F0.

**Target Platform**: Windows 11 primero (entorno actual). Tauri es
multiplataforma; no introducir código específico de Windows salvo manejo de
rutas.

**Project Type**: Aplicación de escritorio (Tauri: core Rust + frontend web).

**Performance Goals**: Reflejar el primer evento real del agente en la UI
<10 s tras arrancarlo (SC-002); dos agentes en paralelo sin bloqueo de UI.

**Constraints**: Cero API keys, cero almacenamiento de credenciales (principio
I, SC-005). Nada llega a la rama principal sin aprobación explícita del usuario.

**Scale/Scope**: 1 usuario local, 2 agentes concurrentes en F0 (límite de
concurrencia configurable, default 2), N tareas en cola.

## Constitution Check

*GATE: revisado antes y después del diseño.*

| Principio | Cumplimiento en este plan |
|---|---|
| I. Solo CLIs oficiales, cero API/credenciales | Solo se ejecutan `claude`/`codex` ya logueados; la app nunca lee ni guarda tokens. La detección de auth se hace ejecutando un comando de estado de la CLI, no leyendo sus archivos de sesión. ✅ |
| II. Headless estructurado, no scraping | Se usan `--output-format stream-json` y `--json`; la TUI nunca se invoca. ✅ |
| III. Aislamiento por worktree | Un worktree + rama `nexora/<task-id>` por tarea; el agente corre con cwd/`-C` apuntando a él. ✅ |
| IV. Coordinación mediada con aprobación del jefe | En F0 la coordinación es manual; la aprobación del jefe es el gate de merge (FR-008). El handshake entre agentes y el planificador llegan en P3/Fase 2. ✅ |
| V. Simplicidad primero | Monaco, xterm, XState, MCP server y decomposición automática se DIFIEREN. Estado = enum + columna en SQLite, no máquina de estados formal todavía. rusqlite en vez de ORM pesado. ✅ |

Sin violaciones que justificar → tabla de complejidad vacía.

## Decisiones de diseño (research)

### D1 — Autonomía del agente es segura PORQUE el worktree está aislado
Para correr headless sin prompts, cada agente necesita permiso de edición
autónoma:
- Codex: `codex exec -s workspace-write` con política de aprobación no
  interactiva (config `approval_policy`), cwd vía `-C <worktree>`.
- Claude: `claude -p --permission-mode acceptEdits` (o equivalente) con cwd =
  worktree.

Esto es aceptable **no** porque confiemos ciegamente en el agente, sino porque
sus cambios viven en un worktree aislado y **nada llega a la rama principal sin
el merge que el usuario aprueba**. La autonomía está contenida por git, no por
prompts. (Decisión clave de seguridad.)

### D2 — La lista de "archivos modificados" es `git status`, no lo que dice el agente
El panel de archivos modificados se deriva de `git status --porcelain` en el
worktree (verdad de terreno), no de los eventos `tool_use` del agente (que
pueden mentir o fallar). Los eventos del stream alimentan el *feed de
actividad*; git alimenta el *estado real de archivos*. Alinea con el principio
de no propagar afirmaciones del agente como verdad.

### D3 — Modelo de eventos unificado (el corazón del adaptador)
Cada adaptador normaliza el JSONL crudo de su CLI a un enum común:

```
AgentEvent =
  | Started   { task_id }
  | Step      { text }                 // mensaje/razonamiento del agente
  | ToolUse   { name, summary }        // herramienta/comando invocado
  | TokenUsage{ input, output, cost? } // si la CLI lo reporta
  | Done      { summary, success }
  | Error     { message }
  | Raw       { json }                 // evento desconocido -> log, NO métrica
```

Mapeo:
- Claude `{"type":"system","subtype":"init"}` → `Started`;
  `{"type":"assistant", content:[text|tool_use]}` → `Step`/`ToolUse`;
  `{"type":"result", total_cost_usd, usage}` → `TokenUsage` + `Done`.
- Codex `--json`: eventos de mensaje del agente → `Step`; ejecución de
  comandos/herramientas → `ToolUse`; conteo de tokens → `TokenUsage`; mensaje
  final / `--output-last-message` → `Done`.
- Cualquier `type` no reconocido → `Raw` (registrado, nunca renderizado como
  porcentaje ni métrica inventada — principio II).

NEEDS CAPTURE: el esquema exacto de `codex exec --json` se captura en una tarea
de implementación (correr una vez, guardar muestra) porque varía por versión.
El parser se escribe defensivo (campos opcionales, `Raw` como fallback).

### D4 — Concurrencia y backpressure
Cola de tareas con límite de concurrencia configurable (default 2). Cada tarea
= un proceso hijo + un worktree. tokio lee stdout línea a línea y emite eventos
Tauri al frontend; la UI nunca se bloquea. Cancelar = matar el proceso hijo +
marcar la tarea y dejar el worktree para inspección/limpieza.

### D5 — Estado: enum + columna, no máquina de estados formal (todavía)
`TaskStatus = Pending | Approved | Running | Reviewing | Done | Failed |
Cancelled`. Transiciones validadas con una función simple en Rust.
<!-- ponytail: enum + guard simple; meter XState/máquina formal solo si las
transiciones se vuelven realmente ramificadas en Fase 2 -->

### D6 — Detección de agentes sin tocar credenciales
Disponibilidad: `which`/PATH. Estado de login: ejecutar un comando de estado
ligero de cada CLI (p.ej. el que reporta auth/whoami sin abrir sesión
interactiva) y leer su código de salida/salida. Nunca leer los archivos de
sesión (`~/.codex`, `~/.claude`) ni copiarlos. Si no hay forma no interactiva
de saber el estado de login, asumir "desconocido" y dejar que el primer `exec`
revele el fallo de auth (mostrando el error real de la CLI). NEEDS CAPTURE: el
comando exacto de estado de cada CLI.

## Project Structure

### Documentation (this feature)

```text
specs/001-mvp-plomeria/
├── spec.md          # hecho
├── plan.md          # este archivo
├── data-model.md    # entidades y esquema SQLite
└── tasks.md         # lo genera /speckit-tasks (siguiente paso)
```

### Source Code (repository root)

```text
src-tauri/                      # core Rust (Tauri v2)
├── Cargo.toml
├── tauri.conf.json
└── src/
    ├── main.rs                 # bootstrap Tauri, registro de comandos
    ├── agents/
    │   ├── mod.rs              # trait AgentAdapter
    │   ├── claude.rs           # adaptador Claude Code (claude -p stream-json)
    │   ├── codex.rs            # adaptador Codex CLI (codex exec --json)
    │   └── events.rs           # enum AgentEvent + parser/normalización
    ├── runner.rs               # spawn de procesos, lectura de stdout, cola/concurrencia
    ├── worktree.rs             # git worktree add/remove, status, diff, merge
    ├── detect.rs               # detección de CLIs y estado de auth (D6)
    ├── db.rs                   # rusqlite: esquema, inserción de eventos/tareas
    └── commands.rs             # comandos Tauri expuestos al frontend
                                # (open_project, list_agents, start_task,
                                #  cancel_task, get_diff, merge_task, ...)

src/                            # frontend React + TS + Vite
├── main.tsx
├── App.tsx
├── pages/
│   ├── Dashboard.tsx           # estado de agentes, proyecto, tareas recientes
│   └── Workspace.tsx           # paneles de agentes + tareas + diff
├── components/
│   ├── AgentPanel.tsx          # estado, feed de actividad, archivos (git status)
│   ├── TaskForm.tsx            # crear tarea + asignación manual de agente
│   └── DiffView.tsx            # diff del worktree + botón aprobar/merge
├── lib/
│   └── ipc.ts                  # wrappers de invoke + suscripción a eventos Tauri
└── styles/                     # tokens de diseño (fase UI/UX Pro Max)

tests/  -> cargo tests viven en src-tauri/src/*.rs (#[cfg(test)])
```

**Structure Decision**: App de escritorio Tauri. Toda la orquestación
(procesos, worktrees, DB) vive en Rust (`src-tauri`), porque Rust lanza y lee
las CLIs nativamente — no hace falta un sidecar Node en F0. El frontend React
solo dibuja estado y dispara comandos vía IPC. El `trait AgentAdapter` se
respeta desde el día 1 aunque solo haya 2 implementaciones (es el punto de
extensión para Gemini/Aider en Fase 4; no es abstracción especulativa porque ya
hay 2 implementaciones reales que lo justifican).

## Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| Esquema de `codex exec --json` cambia por versión | Parser defensivo + fallback `Raw`; tarea de captura de muestra antes de codificar el parser. |
| Agente se cuelga esperando aprobación pese a flags | D1: fijar modo no interactivo explícito; timeout por tarea; cancelación dura. |
| Worktree sobre repo con cambios sin commitear | Advertir y exigir árbol limpio antes de crear worktree (edge case del spec). |
| Conflicto lógico de contrato entre dos tareas | F0 lo resuelve en merge manual; contract-first automático es Fase 2 (documentado, no resuelto aquí). |
| Rutas Windows en worktrees/git | Usar APIs de path de Rust; probar con rutas con espacios. |
| Coste/rate limit con 2 agentes a la vez | Límite de concurrencia configurable (default 2); mostrar tokens reales por tarea. |

## Verificación mínima (quality gate)

- `cargo test` en `agents/events.rs`: alimentar líneas JSONL de muestra (Claude
  y Codex) y afirmar que se normalizan a los `AgentEvent` correctos, incluido
  un evento desconocido → `Raw`.
- `cargo test` en `worktree.rs`: sobre un repo git temporal, crear worktree,
  escribir un archivo, afirmar que `status` lo detecta y que `merge` lo lleva a
  la rama principal.
- Revisión manual obligatoria: grep del código para garantizar que no existe
  lectura/escritura de credenciales ni endpoints de API de proveedores (SC-005).

## Next steps

1. `/speckit-tasks` → desglose accionable y ordenado por dependencias.
2. Fase de diseño UI/UX (UI UX Pro Max) para Dashboard/Workspace antes de
   construir los componentes React.
3. Instalar Rust (`rustup`) para habilitar el build de Tauri.
