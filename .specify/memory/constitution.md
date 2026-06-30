# Nexora Studio Constitution

Nexora Studio es un orquestador de escritorio que coordina agentes de IA de
línea de comandos (inicialmente Codex CLI y Claude Code) para que colaboren
sobre un mismo proyecto, como un equipo humano coordinado por un jefe.

## Core Principles

### I. Solo herramientas oficiales, cero APIs, cero credenciales
Nexora NUNCA usa APIs de pago ni almacena tokens, contraseñas o sesiones.
Únicamente ejecuta las CLIs oficiales ya instaladas y autenticadas por el
usuario con su cuenta personal (`claude -p`, `codex exec`). La autenticación
es responsabilidad de cada CLI; Nexora la hereda, no la gestiona, no la
intercepta. Si una funcionalidad exige una API key o saltarse el login
oficial, no se construye.

### II. Headless estructurado, no scraping de terminal
La integración con cada agente se hace por su modo no interactivo oficial y su
salida estructurada (`--output-format stream-json` en Claude; `codex exec` y
`codex mcp-server`). Prohibido parsear la TUI interactiva o PTYs. Toda la UI de
estado (archivos tocados, herramientas usadas, tokens, paso actual) se deriva
de esos eventos JSON. No se inventan métricas: no hay barras de progreso
ficticias; se muestra estado real o no se muestra.

### III. Aislamiento por git worktree
Cada agente trabaja en su propio git worktree. Nunca dos agentes escriben en el
mismo working directory a la vez. Los conflictos se resuelven en la integración
deliberada (merge), no con bloqueos de archivo en vivo. Antes de paralelizar
trabajo dependiente se define primero el contrato (contract-first): tipos,
endpoints, nombres. El paralelismo real solo aplica a ramas independientes del
grafo de tareas.

### IV. Coordinación mediada con revisión entre pares (modelo equipo humano)
Los agentes NO se comunican directamente entre sí con texto libre. El
orquestador media: mantiene el estado compartido y enruta mensajes. El flujo
imita a un equipo: un agente propone una división del trabajo, el otro la
revisa ("¿está bien esto?"), y el usuario es el jefe que aprueba el plan antes
de cualquier ejecución. Cuando un agente termina algo que afecta a otro, hay un
handshake de revisión antes de que el trabajo dependiente continúe. Ningún
resumen generado por un agente se propaga como verdad sin un paso de
verificación.

### V. Simplicidad primero, el cerebro después (YAGNI)
Se construye por fases. La Fase 0 (plomería: dos agentes en paralelo sobre
worktrees, asignación manual, UI de streams en vivo, merge) debe funcionar y
sentirse bien antes de invertir en la decomposición automática de tareas o en
recrear un IDE. No se reconstruye VS Code: se embebe Monaco para vista/edición
ligera y se delega la edición pesada a VS Code. No se añade abstracción
especulativa (un solo agente todavía no justifica un sistema de plugins
completo, pero el contrato de adaptador se respeta desde el día 1).

## Arquitectura y stack

- Shell de escritorio: **Tauri** (core Rust). Rust lanza y lee las CLIs por
  subprocess de forma nativa.
- Frontend: **React + TypeScript + Vite**. Diseño con la skill UI/UX Pro Max;
  estética IDE moderno.
- Editor: **Monaco** embebido + "Abrir en VS Code". Terminal: **xterm.js**.
- Estado de agentes: máquina de estados (idle → planning → awaiting-approval →
  running → reviewing → merging → done/error).
- Persistencia: **SQLite** (tareas, eventos, historial).
- Memoria/coordinación compartida: archivos nativos que las CLIs ya leen
  (`CLAUDE.md`, `AGENTS.md`) + un **MCP server propio de Nexora** para el estado
  vivo (contrato actual, tareas, decisiones). Ambos agentes se conectan a él.
- Adaptador de agentes: interfaz `AgentAdapter { id, name, capabilities[],
  start(task, sharedContext) -> stream<AgentEvent>, cancel() }`. Codex y Claude
  son implementaciones; agentes futuros (Gemini, Aider, OpenHands) son nuevos
  adaptadores. El core nunca habla con una CLI directamente.

## Quality gates

- Toda lógica no trivial (parser de eventos, planificador, merge, money/security
  paths) deja al menos una verificación ejecutable mínima.
- Ningún cambio puede introducir almacenamiento de credenciales ni llamadas a
  APIs de los proveedores. Revisión obligatoria de este punto.
- El usuario (jefe) aprueba explícitamente cualquier plan antes de que los
  agentes ejecuten cambios en archivos.

## Governance

Esta constitución prevalece sobre cualquier otra práctica. Cualquier desviación
(especialmente de los principios I, III y IV) debe justificarse por escrito en
el plan de la feature correspondiente. La complejidad debe justificarse; en la
duda, gana la opción más simple que funcione.

**Version**: 1.0.0 | **Ratified**: 2026-06-29 | **Last Amended**: 2026-06-29
