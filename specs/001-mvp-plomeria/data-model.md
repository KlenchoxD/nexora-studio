# Data Model — MVP Fase 0

SQLite local (rusqlite). Un archivo por instalación. Solo datos operativos;
**nunca** credenciales ni tokens.

## Entidades

### project
Repo git abierto en Nexora.
| campo | tipo | notas |
|---|---|---|
| id | TEXT PK | uuid |
| path | TEXT | ruta absoluta del repo |
| base_branch | TEXT | rama de integración (p.ej. `main`) |
| opened_at | INTEGER | epoch |
| last_active_at | INTEGER | para "proyectos recientes" en el Dashboard |

### agent
Catálogo de adaptadores disponibles (sembrado: `claude`, `codex`).
| campo | tipo | notas |
|---|---|---|
| id | TEXT PK | `claude` / `codex` |
| name | TEXT | "Claude Code" / "Codex CLI" |
| capabilities | TEXT (json) | p.ej. `["frontend","ui","review"]` / `["backend","tests","refactor"]` — configurable |
| status | TEXT | derivado en runtime (no persistido como verdad): installed / logged_out / ready / busy / error / unknown |

`status` se recalcula al abrir proyecto (D6 del plan); en DB solo cachea el último valor visto.

### task
Unidad de trabajo asignada a un agente.
| campo | tipo | notas |
|---|---|---|
| id | TEXT PK | uuid; usado en rama `nexora/<id>` |
| project_id | TEXT FK | |
| agent_id | TEXT FK | asignación manual en F0 |
| description | TEXT | la instrucción para el agente |
| status | TEXT | Pending / Approved / Running / Reviewing / Done / Failed / Cancelled |
| worktree_path | TEXT | ruta del worktree, null hasta crearse |
| branch | TEXT | `nexora/<id>` |
| depends_on | TEXT (json) | ids de tareas previas; vacío en F0 (asignación manual) |
| created_at | INTEGER | |
| started_at | INTEGER | null hasta ejecutar |
| ended_at | INTEGER | null hasta terminar |
| cost_usd | REAL | acumulado de TokenUsage si la CLI lo reporta |
| error | TEXT | mensaje real de la CLI si Failed |

### agent_event
Feed de actividad normalizado (append-only). Alimenta los paneles en vivo y el historial.
| campo | tipo | notas |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| task_id | TEXT FK | |
| kind | TEXT | started / step / tool_use / token_usage / done / error / raw |
| payload | TEXT (json) | datos del evento |
| ts | INTEGER | epoch ms |

## Notas de integridad
- **Archivos modificados NO se almacenan como entidad** — se leen on-demand con
  `git status --porcelain` sobre `worktree_path` (D2 del plan: git es la verdad,
  no los eventos del agente).
- `agent_event.kind = 'raw'` captura eventos desconocidos sin romper la UI ni
  convertirse en métrica (principio II: sin métricas inventadas).
- Borrar una tarea limpia su worktree (`git worktree remove`) pero conserva sus
  `agent_event` para historial salvo que el usuario purgue.

## Transiciones de estado (task.status)
```
Pending --aprobar--> Approved --lanzar--> Running --fin ok--> Reviewing --merge--> Done
                                              |--fin error--> Failed
                                              |--cancelar---> Cancelled
```
En F0 sin propuesta de plan, una tarea de asignación manual entra directa a
`Approved` al crearse (el "jefe" la creó). El gate de aprobación explícito
aplica al **merge** (Reviewing → Done) y, en P3, a la propuesta de división.
