---
description: "Task list — MVP Fase 0 Nexora Studio"
---

# Tasks: MVP Fase 0 — Plomería de orquestación paralela

**Input**: `specs/001-mvp-plomeria/` (spec.md, plan.md, data-model.md)

**Tests**: incluidos solo donde el plan los exige como quality gate (parser de
eventos y gestor de worktrees). No se testea la UI en F0.

## Format: `[ID] [P?] [Story] Description`
- **[P]**: paralelizable (archivos distintos, sin dependencias entre sí)
- **[Story]**: US1–US5 según spec.md

---

## Phase 1: Setup (infraestructura compartida)

- [ ] T001 Instalar Rust con `rustup` y verificar `cargo --version` (prerequisito de Tauri)
- [ ] T002 Scaffold Tauri v2 + React + TS + Vite en la raíz del repo (`src-tauri/` + `src/`); arrancar app vacía con `cargo tauri dev`
- [ ] T003 [P] Añadir dependencias Rust en `src-tauri/Cargo.toml`: tokio, serde, serde_json, rusqlite (feature `bundled`), uuid
- [ ] T004 [P] Configurar formato/lint: `rustfmt` + `clippy` (Rust) y ESLint + Prettier (frontend)
- [ ] T005 [P] Añadir `.claude/` y artefactos de sesión a `.gitignore`

---

## Phase 2: Foundational (BLOQUEA todas las historias)

**⚠️ Ninguna historia puede empezar hasta completar esta fase.**

- [ ] T006 [CAPTURE] Correr una vez `codex exec --json "di hola"` y `claude -p --output-format stream-json "di hola"` en un repo de prueba; guardar las muestras JSONL en `specs/001-mvp-plomeria/samples/` (insumo para el parser; resuelve el NEEDS CAPTURE del plan)
- [ ] T007 [CAPTURE] Determinar el comando no interactivo de estado de auth de cada CLI (D6); documentar en `plan.md` o `samples/`
- [ ] T008 Crear esquema SQLite en `src-tauri/src/db.rs` (tablas `project`, `agent`, `task`, `agent_event` según data-model.md) + sembrado de `agent` (claude, codex)
- [ ] T009 Definir `enum AgentEvent` y el `enum TaskStatus` en `src-tauri/src/agents/events.rs`
- [ ] T010 Definir `trait AgentAdapter { id, name, capabilities, start(task, cwd) -> stream<AgentEvent>, cancel() }` en `src-tauri/src/agents/mod.rs`
- [ ] T011 Implementar `runner.rs`: spawn de proceso hijo con tokio, cwd configurable, lectura de stdout línea a línea, emisión de eventos Tauri al frontend, kill/cancel
- [ ] T012 Implementar `worktree.rs`: `add(branch)`, `remove`, `status` (`git status --porcelain`), `diff`, `merge(into base)` sobre el repo del proyecto
- [ ] T013 [P] [TEST] `cargo test` en `worktree.rs`: sobre repo git temporal, crear worktree → escribir archivo → `status` lo detecta → `merge` lo lleva a la rama base
- [ ] T014 Plomería IPC en `src-tauri/src/commands.rs` + `src/lib/ipc.ts`: comando `open_project`, suscripción a eventos de agente desde React

**Checkpoint**: base lista (DB, eventos, adaptador, runner, worktrees, IPC).

---

## Phase 3: User Story 1 — Estado de agentes en el Dashboard (P1) 🎯 MVP

**Goal**: abrir un repo y ver Codex/Claude con su estado sin pedir credenciales.

- [ ] T015 [US1] Implementar `detect.rs`: disponibilidad por PATH + estado de auth vía comando de T007, mapeado a `installed/logged_out/ready/unknown` (sin leer archivos de sesión)
- [ ] T016 [US1] Comando Tauri `list_agents(project)` + `open_project(path)` (con oferta de `git init` si no es repo)
- [ ] T017 [US1] `pages/Dashboard.tsx`: proyectos recientes, estado de cada agente (🟢/🟠/⚪), proyecto abierto
- [ ] T018 [US1] `components/AgentPanel.tsx` (esqueleto): nombre, estado, capacidades

**Checkpoint**: el Dashboard refleja el estado real de ambos agentes (SC-001, SC-005).

---

## Phase 4: User Story 2 — Lanzar tarea y ver stream en vivo (P1) 🎯 MVP

**Goal**: asignar una tarea a un agente y ver su actividad real en su panel.

- [ ] T019 [US2] Adaptador `agents/claude.rs`: construir invocación `claude -p --output-format stream-json --permission-mode acceptEdits` con cwd=worktree; normalizar a `AgentEvent`
- [ ] T020 [US2] Adaptador `agents/codex.rs`: construir invocación `codex exec --json -s workspace-write -C <worktree>` (aprobación no interactiva); normalizar a `AgentEvent`
- [ ] T021 [US2] [TEST] `cargo test` en `agents/events.rs`: alimentar muestras JSONL de T006 (Claude y Codex) → afirmar normalización correcta + evento desconocido → `Raw`
- [ ] T022 [US2] Comando `start_task(description, agent_id)`: crear task (estado Approved), crear worktree (T012), lanzar adaptador vía runner, persistir eventos
- [ ] T023 [US2] Comando `cancel_task(id)`: matar proceso + marcar Cancelled (deja worktree para inspección)
- [ ] T024 [US2] `components/TaskForm.tsx`: input de tarea + selector manual de agente
- [ ] T025 [US2] `AgentPanel.tsx`: feed de actividad en vivo (Step/ToolUse), tokens reales si los hay, botón cancelar — **sin barra de % ficticia**
- [ ] T026 [US2] Panel de "archivos modificados" = `git status --porcelain` del worktree (D2), no eventos del agente

**Checkpoint**: una tarea se ejecuta headless y su trabajo real aparece en la UI (SC-002).

---

## Phase 5: User Story 3 — Dos agentes en paralelo sin pisarse (P1) 🎯 MVP

**Goal**: Codex y Claude trabajando a la vez, cambios aislados por worktree.

- [ ] T027 [US3] Cola de tareas con límite de concurrencia configurable (default 2) en `runner.rs`
- [ ] T028 [US3] `pages/Workspace.tsx`: dos `AgentPanel` lado a lado avanzando en paralelo + panel de tareas
- [ ] T029 [US3] Verificación de aislamiento: cada tarea usa su propia rama `nexora/<id>` y worktree; advertir si el repo tiene cambios sin commitear antes de crear worktree (edge case)

**Checkpoint**: dos tareas independientes corren a la vez, cambios en worktrees separados (SC-003). **MVP demostrable aquí.**

---

## Phase 6: User Story 4 — Aprobación del jefe e integración por merge (P2)

**Goal**: revisar el diff de un worktree y mergear o descartar.

- [ ] T030 [US4] Comando `get_diff(task_id)` (usa `worktree.diff`) y `merge_task(task_id)` (estado Reviewing → Done, merge a base, `git worktree remove`)
- [ ] T031 [US4] Comando `discard_task(task_id)`: eliminar worktree y rama sin mergear
- [ ] T032 [US4] `components/DiffView.tsx`: mostrar diff + botones Aprobar(merge)/Descartar
- [ ] T033 [US4] Manejo de conflicto de merge: detectar, informar claramente, dejar resolución manual en F0 (edge case)

**Checkpoint**: ciclo completo crear→ejecutar→revisar→integrar (SC-004).

---

## Phase 7: User Story 5 — Propuesta de división revisable (P3)

**Goal**: proponer subtareas + agente sugerido, revisables, sin ejecutar hasta aprobar.

- [ ] T034 [US5] Comando `propose_plan(request)`: usar un agente en modo planificador (`claude -p --json-schema` o `codex exec --output-schema`) para devolver subtareas + agente sugerido + dependencias en JSON; persistir como tasks en estado `Pending`
- [ ] T035 [US5] UI de propuesta: lista de subtareas editable (reasignar agente/orden), nada se ejecuta en `Pending`
- [ ] T036 [US5] Acción "Aprobar plan" (jefe): pasa las tasks de `Pending` a `Approved` y las encola (respetando `depends_on`)

**Checkpoint**: flujo proponer → revisar → aprobar → ejecutar (FR-010).

---

## Phase 8: Polish & cross-cutting

- [ ] T037 [P] Pantalla de Logs (eventos `raw` y errores crudos de las CLIs)
- [ ] T038 [P] Persistir/mostrar consumo de tokens y coste por tarea en el Dashboard
- [ ] T039 Revisión de seguridad: grep del repo confirmando cero API keys / endpoints de proveedores / lectura de archivos de credenciales (SC-005)
- [ ] T040 Manejo de errores: sesión expirada a mitad de tarea → mostrar error real de la CLI y marcar Failed (edge case)

---

## Dependencies & Execution Order

- **Phase 1 Setup** → sin dependencias.
- **Phase 2 Foundational** → depende de Setup; **bloquea US1–US5**. T006/T007 (CAPTURE) deben ir antes de T019–T021 (adaptadores/parser).
- **US1, US2, US3 (todas P1)** → tras Foundational. US2 depende de los adaptadores (T019/T020); US3 depende de US2 (necesita poder lanzar una tarea antes de lanzar dos).
- **US4 (P2)** → tras US2 (necesita tareas que produzcan diffs).
- **US5 (P3)** → tras US2/US3 (necesita ejecución funcionando).
- **Polish** → al final.

### Paralelizable
- Setup: T003, T004, T005.
- Foundational: T013 (test) en paralelo con desarrollo de otras piezas; T011 y T012 son módulos independientes.
- US2: T019 y T020 (adaptadores distintos) en paralelo.
- Polish: T037, T038.

## Implementation Strategy

**MVP = Setup + Foundational + US1 + US2 + US3.** Parar ahí y validar: dos
agentes trabajando en paralelo sobre worktrees aislados, con estado real en la
UI. Si eso se siente bien, el producto vale. US4 y US5 son incrementos.

## Notes
- T001–T002 son el único momento de "setup pesado" (instalar Rust, scaffold).
- Los dos únicos tests obligatorios: T013 (worktrees) y T021 (parser). El resto
  se valida manualmente por historia.
- Commit por tarea o grupo lógico. Parar en cualquier checkpoint a validar.
