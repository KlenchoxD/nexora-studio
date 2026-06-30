# Feature Specification: MVP Fase 0 — Plomería de orquestación paralela

**Feature Branch**: `001-mvp-plomeria`

**Created**: 2026-06-29

**Status**: Draft

**Input**: Orquestar Codex CLI y Claude Code en paralelo sobre un mismo
proyecto, con aislamiento por git worktree, asignación manual de tareas, UI
unificada de streams en vivo, e integración por merge. Cuentas personales, sin
API. Coordinación tipo equipo humano con aprobación del usuario (jefe).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Abrir proyecto y ver estado de ambos agentes (Priority: P1)

El usuario abre Nexora Studio, selecciona una carpeta de proyecto (un repo git)
y ve un dashboard con: estado de Codex (instalado/logueado), estado de Claude
(instalado/logueado), el proyecto abierto y un panel de agentes vacío listo
para recibir tareas.

**Why this priority**: Sin detección de las CLIs y su estado de login, nada más
funciona. Es la base que prueba el principio "solo herramientas oficiales".

**Independent Test**: Abrir un repo git y verificar que la app reporta
correctamente si `claude` y `codex` están instalados y autenticados, sin pedir
ninguna credencial.

**Acceptance Scenarios**:

1. **Given** `claude` y `codex` instalados y logueados, **When** abro un repo
   git, **Then** el dashboard muestra ambos como "🟢 listo" sin solicitar API
   keys ni contraseñas.
2. **Given** `codex` no está logueado, **When** abro el proyecto, **Then** el
   dashboard muestra Codex como "🟠 requiere login" con instrucción de correr
   `codex login` (Nexora no gestiona el login, solo informa).
3. **Given** una carpeta que no es repo git, **When** la abro, **Then** la app
   ofrece ejecutar `git init` antes de continuar.

---

### User Story 2 - Lanzar una tarea a un agente y ver su stream en vivo (Priority: P1)

El usuario escribe una tarea, la asigna manualmente a Codex o a Claude, y ve en
el panel de ese agente el stream en vivo: paso actual, herramientas usadas y
archivos modificados, todo derivado de la salida JSON estructurada del agente.

**Why this priority**: Es el corazón del MVP. Prueba que podemos dirigir las
CLIs en headless y reflejar su trabajo real en la UI.

**Independent Test**: Asignar "crea un archivo HELLO.md con un saludo" a un
agente y verificar que el panel muestra los eventos en vivo y el archivo
aparece modificado en el worktree de ese agente.

**Acceptance Scenarios**:

1. **Given** un agente listo, **When** le asigno una tarea, **Then** se lanza
   en modo headless (`claude -p --output-format stream-json` o `codex exec`) en
   su propio git worktree.
2. **Given** una tarea en ejecución, **When** el agente usa una herramienta o
   modifica un archivo, **Then** el panel del agente lo refleja en tiempo real
   a partir de eventos JSON (sin métricas inventadas).
3. **Given** una tarea en ejecución, **When** pulso "cancelar", **Then** el
   proceso del agente se detiene y el estado pasa a "cancelado".

---

### User Story 3 - Dos agentes en paralelo sin pisarse (Priority: P1)

El usuario asigna una tarea a Codex y otra a Claude simultáneamente. Cada uno
trabaja en su propio worktree. Ambos paneles avanzan a la vez y ninguno
modifica los archivos del otro.

**Why this priority**: Es la promesa central del producto: colaboración
simultánea sin conflictos. Prueba el principio de aislamiento por worktree.

**Independent Test**: Lanzar dos tareas que tocan archivos distintos a la vez y
verificar que ambos worktrees avanzan en paralelo y los cambios quedan
aislados.

**Acceptance Scenarios**:

1. **Given** dos tareas independientes, **When** las lanzo a la vez, **Then**
   se crean dos worktrees separados y ambos agentes ejecutan en paralelo.
2. **Given** ambos trabajando, **When** reviso los archivos, **Then** los
   cambios de cada agente solo existen en su propio worktree.

---

### User Story 4 - Aprobación del jefe e integración por merge (Priority: P2)

Cuando un agente termina, el usuario revisa el diff de su worktree y decide si
integrarlo (merge a la rama principal) o descartarlo. El usuario es el jefe que
aprueba.

**Why this priority**: Cierra el ciclo de trabajo. Sin integración, el trabajo
queda atrapado en worktrees. Es P2 porque P1 ya entrega valor demostrable
(ver agentes trabajar en paralelo) aunque la integración sea manual vía git.

**Independent Test**: Terminar una tarea, revisar el diff en la app y hacer
merge; verificar que los cambios llegan a la rama principal.

**Acceptance Scenarios**:

1. **Given** una tarea terminada, **When** abro su resultado, **Then** veo el
   diff del worktree antes de decidir.
2. **Given** un diff aprobado, **When** pulso "integrar", **Then** se hace merge
   a la rama principal y se limpia el worktree.
3. **Given** un merge con conflicto, **When** ocurre, **Then** la app lo informa
   claramente y ofrece resolverlo (manual en esta fase).

---

### User Story 5 - Propuesta de división con revisión y aprobación (Priority: P3)

El usuario escribe una petición grande. Un agente propone cómo dividirla en
subtareas y a qué agente asignar cada una. El otro agente (o el orquestador)
revisa la propuesta. El usuario aprueba o ajusta antes de ejecutar.

**Why this priority**: Es el primer paso hacia el "cerebro", pero el MVP es
viable sin él (asignación manual ya funciona). Modela el flujo de equipo
humano: proponer → revisar → aprobar el jefe → ejecutar.

**Independent Test**: Dar una petición y verificar que se genera un plan de
subtareas revisable y editable, que no se ejecuta nada hasta la aprobación.

**Acceptance Scenarios**:

1. **Given** una petición, **When** la envío, **Then** se genera una propuesta
   de subtareas con agente sugerido y dependencias, en estado "pendiente de
   aprobación".
2. **Given** una propuesta, **When** la reviso, **Then** puedo editar
   asignaciones y orden antes de aprobar.
3. **Given** una propuesta no aprobada, **When** está pendiente, **Then** ningún
   agente modifica archivos.

---

### Edge Cases

- ¿Qué pasa si una CLI no está instalada? → estado "no instalado" con guía, sin
  romper la app.
- ¿Qué pasa si la sesión de la cuenta expira a mitad de tarea? → el agente
  fallará; la app muestra el error real de la CLI y marca la tarea como fallida.
- ¿Qué pasa si el repo tiene cambios sin commitear al crear un worktree? → la
  app advierte antes de crear el worktree.
- ¿Qué pasa si dos tareas, pese a estar en worktrees, modifican lógicamente el
  mismo contrato? → el conflicto aparece en el merge; en esta fase se resuelve
  manualmente (contract-first es Fase 2).
- ¿Qué pasa si el agente emite JSON malformado o un evento desconocido? → se
  registra crudo en logs y no se rompe la UI.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: El sistema DEBE detectar si `claude` y `codex` están instalados y
  reportar su estado de autenticación SIN pedir ni almacenar credenciales.
- **FR-002**: El sistema DEBE ejecutar los agentes únicamente vía su modo
  headless oficial (`claude -p --output-format stream-json`, `codex exec`) y
  NUNCA vía API key ni parseo de la TUI interactiva.
- **FR-003**: El sistema DEBE crear un git worktree dedicado por tarea/agente y
  ejecutar al agente dentro de él.
- **FR-004**: El sistema DEBE parsear el stream JSON de cada agente y mostrar en
  su panel: estado, paso/tarea actual, herramientas usadas y archivos
  modificados, sin inventar métricas (sin barra de % ficticia).
- **FR-005**: Los usuarios DEBEN poder asignar manualmente una tarea a un agente
  concreto.
- **FR-006**: El sistema DEBE poder ejecutar dos agentes en paralelo con
  aislamiento total de working directory.
- **FR-007**: Los usuarios DEBEN poder cancelar una tarea en ejecución.
- **FR-008**: El sistema DEBE mostrar el diff de un worktree y permitir al
  usuario (jefe) aprobar el merge a la rama principal o descartarlo.
- **FR-009**: El sistema DEBE persistir tareas, eventos e historial localmente.
- **FR-010**: El sistema NO DEBE ejecutar cambios en archivos sin aprobación
  explícita del usuario para el plan (cuando exista propuesta de división).

### Key Entities *(include if feature involves data)*

- **Project**: carpeta/repo git abierto; ruta, rama base, estado.
- **Agent**: una CLI integrada; id, nombre, capacidades, estado
  (instalado/logueado/listo/ocupado/error).
- **Task**: unidad de trabajo; descripción, agente asignado, worktree, estado
  (pendiente/aprobada/ejecutando/terminada/fallida/cancelada), dependencias.
- **AgentEvent**: evento del stream; tipo (tool_use/file_changed/message/done/
  error), payload, timestamp, tarea asociada.
- **Worktree**: directorio git aislado; ruta, rama, tarea asociada, estado de
  merge.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: El usuario puede abrir un repo y ver el estado correcto de ambos
  agentes en menos de 5 segundos, sin introducir ninguna credencial.
- **SC-002**: El usuario puede lanzar una tarea y ver el primer evento real del
  agente reflejado en la UI en menos de 10 segundos tras el arranque del agente.
- **SC-003**: Dos agentes ejecutan en paralelo y, al terminar, sus cambios
  están en worktrees separados sin ninguna colisión de archivos.
- **SC-004**: El usuario puede revisar un diff e integrarlo por merge sin salir
  de la app en el 100% de los casos sin conflicto.
- **SC-005**: En ningún momento la app almacena, transmite ni solicita una API
  key o contraseña de los proveedores (verificable por inspección).

## Assumptions

- El usuario ya tiene `claude` y `codex` instalados y logueados con sus cuentas
  personales (suscripción/login normal), igual que los usa en terminal.
- El proyecto objetivo es (o será inicializado como) un repositorio git.
- La decomposición automática inteligente, la memoria compartida vía MCP, el
  contract-first automático y la estética IDE completa son fases posteriores;
  este MVP usa asignación manual e integración por merge.
- El editor embebido (Monaco) y la terminal embebida (xterm.js) son deseables
  pero pueden diferirse; en Fase 0 basta con ver diffs y streams.
- La plataforma objetivo inicial es Windows (entorno actual del usuario).
