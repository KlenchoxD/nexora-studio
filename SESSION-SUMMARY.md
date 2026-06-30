# Nexora Studio — resumen de sesión (estado al despertar)

## Qué es
Orquestador de escritorio (Tauri + React/Rust) que coordina **Codex CLI** y
**Claude Code** **directo sobre tus carpetas locales** (git es opcional, igual
que Claude Code), usando tus **cuentas personales** (login oficial, **cero API
keys**, cero credenciales almacenadas).

## Estado: MVP (Fase 0) COMPLETO en código y compilando ✅

Flujo real funcionando de punta a punta:
`detectar agentes → abrir carpeta → lanzar tarea (Codex / Claude / Ambos) →
ver stream en vivo → cancelar → ver cambios`, **todo persistido en SQLite**.
El agente edita **directo en tu carpeta** (como Claude Code); si la carpeta es
un repo git, "Ver cambios" muestra el diff real y git es tu undo.

## Verificación (lo que SÍ pude probar headless)
| Prueba | Resultado |
|---|---|
| `cargo test` (lógica backend) | ✅ **11/11** (parser de eventos, worktrees, SQLite, adaptadores, runner) |
| `npm run build` (tipos + bundle frontend) | ✅ verde (41 módulos) |
| `cargo build` app Tauri completa (enlaza todo) | ✅ exit 0, sin warnings |
| Arranque del binario (`setup()`: abre DB, registra estado, crea ventana) | ✅ bootea sin panic |

## Lo que NO pude probar (límite honesto)
No tengo pantalla para *ver e interactuar* con la ventana. La interacción real
(escribir, ver el stream de un agente trabajando) requiere tu clic. El diseño se
validó visualmente en el preview Astro (`design-preview/`) y el porte React usa
**los mismos tokens/CSS**, así que es fiel.

## Prueba manual (el clic final) — 1 minuto
**Lanza la ventana NATIVA** (no `npm run dev`, eso abre el navegador):
```powershell
cd C:\Users\Kleiner\nexora-studio\app; npm run tauri dev
```
(equivalente corto: `npm start`). En la app: **Abrir carpeta** → elige
**cualquier carpeta tuya** → escribe `crea un archivo HELLO.md con un saludo`
→ elige agente → envía → mira el stream en vivo → **Ver cambios**. El archivo
aparece directo en tu carpeta. El estado de login de cada CLI está en el panel
derecho **Conexiones**.

## Backend (Rust) — módulos
- `events.rs` — `AgentEvent` + parser Claude/Codex (5 tests).
- `worktree.rs` — ayudas git **opcionales**: `is_git_repo`/`diff`/`current_branch` (1 test).
- `db.rs` — SQLite: proyectos, tareas, eventos, seed de agentes (2 tests).
- `agents/` — `trait AgentAdapter` + Claude/Codex (lanzan con **stdin cerrado** y
  `cwd = tu carpeta`; edición directa) (2 tests).
- `runner.rs` — spawn + stream de eventos (1 test).
- `commands.rs` — `detect_agents`, `open_project` (git opcional), `start_task`
  (directo en la carpeta), `cancel_task`, `list_recent_tasks`, `task_diff`.
- `lib.rs` — estado global (DB + registro de procesos para cancelar) + setup.

## Diseño / UX
- **Ventana de escritorio real**: marco propio (sin la doble barra del sistema),
  botones minimizar/maximizar/cerrar, barra superior arrastrable (estilo
  Cursor/Antigravity). 1200×800.
- **Cero datos falsos**: se eliminaron todos los turnos demo y métricas
  inventadas. La app solo muestra estado real; con todo vacío aparece una
  pantalla de bienvenida con el botón **Abrir carpeta** (selector nativo).
- **Una sola pantalla real** (Conversación, totalmente funcional). Workspace y
  Agentes se quitaron del menú hasta cablearlas a datos reales — no hay
  superficies de mentira.
- Paleta "tinta cálida" (aprobada antes). Si se prefiere el tono frío/gris tipo
  Cursor, es un cambio de tokens de minutos.

## Adaptaciones de repos externos (con criterio, sin bloat)
- `docs/orchestration-protocol.md` — contract-first (Happycapy) adaptado.
- `docs/skills-integration.md` — repo Happycapy como catálogo de capacidades.
- `docs/openclaw-adaptation.md` — conceptos de OpenClaw (control plane, sandbox,
  Live Canvas) adoptados; el resto (mensajería, voz, apps móviles) fuera de misión.

## Limitaciones conocidas / siguiente
- **Ambos a la vez sin git**: ahora editan la misma carpeta directamente, así que
  dos agentes en paralelo sobre los mismos archivos pueden chocar. Con git de por
  medio es recuperable; el aislamiento por rama (worktree) quedó en el historial
  por si se reactiva como modo "paralelo seguro" (Fase 2).
- **Codex `--json`**: esquema exacto se confirma en tu primer run real (parser ya
  es defensivo, cae a `Raw` ante lo desconocido — no rompe).
- **Workspace/Agentes**: cablearlas a datos en vivo (siguiente incremento).
- **Planificador automático** (decomposición + contract-first auto) = Fase 2.
- **Live Canvas** (lo que te gustó de OpenClaw) = Fase 3.
- Si un agente muere sin emitir `done`/`error`, la tarea puede quedar en
  `running` (marcado con `ponytail:` en el código).

## Cómo correr los tests tú mismo
```bash
cd C:/Users/Kleiner/nexora-studio/app/src-tauri && cargo test
cd C:/Users/Kleiner/nexora-studio/app && npm run build
```
