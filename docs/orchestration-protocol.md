# Protocolo de orquestación de Nexora (Contract-First Map-Reduce)

> **Adaptado de** [`happycapy-ai/Happycapy-skills` → `contract-first-agents`](https://github.com/happycapy-ai/Happycapy-skills) (licencia MIT).
> Conservamos la atribución. Ajustamos el protocolo a la realidad de Nexora:
> dos CLIs oficiales independientes (Codex y Claude) ejecutando en **git
> worktrees separados**, no subagentes dentro de una sola sesión.

## Por qué lo adoptamos

Ya habíamos elegido **contract-first** en la constitución (definir el contrato
antes de paralelizar). Esta skill aporta una **forma concreta y probada** de
hacerlo: un protocolo de 4 fases tipo map-reduce. Su autor reporta −75% de
errores de integración y +52.5% de calidad frente a coordinación no
estructurada (cifras suyas, no verificadas por nosotros, pero la dirección
coincide con nuestro diseño).

**Diferencia crítica con el original:** su versión coordina subagentes dentro de
**una** sesión de Claude (memoria de proceso compartida). Nexora coordina **dos
procesos separados** sin memoria compartida. Por eso aquí "el contrato ES la
coordinación" es aún más cierto: el **Contract Document + el estado en el MCP**
son el ÚNICO terreno común entre Codex y Claude.

## Las 4 fases (adaptadas a Nexora)

### Fase 1 — Generación del contrato
El planificador (Claude en plan mode, `--json-schema`) produce un **Contract
Document** ANTES de lanzar a ningún agente. Debe ser específico ("usa
snake_case", no "sé consistente"). Contiene:

- **Manifiesto de módulos**: nombres de archivo exactos y símbolos exportados.
- **Interfaces**: firmas de funciones y endpoints, flujos de datos.
- **Tipos compartidos**: nombres de campos y reglas de validación.
- **Guía de estilo**: convenciones de nombres, indentación, formato de docs.
- **Mapa de dependencias**: qué módulo depende de cuál.
- **Fronteras de sección**: qué entrega cada agente y qué importa del otro.

El contrato se guarda en el estado compartido (MCP server de Nexora) y como
archivo que ambos agentes leen (`AGENTS.md` / referenciado desde `CLAUDE.md`).

> **Gate del jefe:** el contrato se presenta en el chat; **tú lo apruebas**
> antes de pasar a ejecución (encaja con tu flujo "equipo humano").

### Fase 2 — Ejecución paralela
Cada agente recibe el **contrato completo** + su asignación de sección, y
ejecuta **a la vez** en su propio worktree:
- Codex → su sección (backend/arquitectura) en `wt-codex`.
- Claude → su sección (frontend/UI) en `wt-claude`.

Map-reduce, no pipeline secuencial: así se conserva el paralelismo real. Las
dependencias entre secciones ya están resueltas por el contrato, no por orden
de ejecución.

### Fase 3 — Validación automática
Antes de integrar, el orquestador valida la salida combinada:
- chequeo de sintaxis · resolución de imports · consistencia de nombres
- completitud (¿está todo lo que el contrato pedía?) · conformidad de estilo
- referencias cruzadas correctas entre secciones
- **+ específico de Nexora:** `git status` real de cada worktree (verdad de
  terreno, no lo que dice el agente) y `typecheck`/tests del contrato.

### Fase 4 — Correcciones dirigidas
Si la validación encuentra problemas, se enruta **solo** lo señalado al agente
dueño de esa sección (un "fixer" puntual), sin regenerar todo. Diff pequeño.

> **Gate del jefe:** tras validar, **tú apruebas el merge** a la rama de
> integración.

## Mapa al plan de Nexora

- Es el corazón de la **Fase 2 (el cerebro)** del roadmap — la decomposición y
  el enrutado automáticos se construyen alrededor de este protocolo.
- En el **MVP (Fase 0)** seguimos con asignación manual; este protocolo entra
  cuando activemos la propuesta automática (US5 → Fase 2).
- El Contract Document es el artefacto que conecta planificador → agentes →
  validación, y lo que se guarda en la memoria compartida.

## Otras skills del repo candidatas (pendiente de tu decisión)

- **capy-cortex** (aprendizaje/memoria persistente) → encaja con nuestra
  **memoria compartida** (MCP + `CLAUDE.md`/`AGENTS.md`): decisiones,
  convenciones y errores que persisten entre sesiones.
- **llm-council** (comparar respuestas de varios modelos) → encaja con tu
  **handshake de revisión** ("que el otro diga si está bien"): un agente
  critica la salida del otro antes de integrar.
- **claude-code-templates / skill-creator** → útiles más adelante para el
  sistema de plugins de agentes (Fase 4).
