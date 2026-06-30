# Integración del repo Happycapy-skills en Nexora

> Fuente: [`happycapy-ai/Happycapy-skills`](https://github.com/happycapy-ai/Happycapy-skills)
> (MIT, 45 skills). Atribución conservada. Aquí decidimos **cómo** se usa cada
> parte en Nexora, sin meter bloat en el core.

## Principio de integración

Nexora ejecuta Claude Code y Codex, que **ya cargan skills nativamente** (de
`~/.claude/skills`, `AGENTS.md`, etc.). Por tanto NO reimplementamos las 45
skills: el repo entero se convierte en un **catálogo de capacidades
instalables** que los agentes de Nexora pueden invocar. "Usar todo el repo" =
soportar el catálogo + adaptar los conceptos de orquestación al diseño.

Tres niveles de uso:

### Nivel 1 — Conceptos que adaptamos al ARQUITECTURA del orquestador
Estos no son capacidades de usuario; son ideas de coordinación que entran en el
diseño de Nexora.

| Skill | Cómo entra en Nexora |
|---|---|
| **contract-first-agents** | Protocolo de orquestación de 4 fases. Ya adaptado en [orchestration-protocol.md](./orchestration-protocol.md). **Fase 2.** |
| **capy-cortex** (memoria persistente) | Capa de **memoria compartida**: decisiones, convenciones, errores que persisten entre sesiones. Se materializa en el MCP server de Nexora + `CLAUDE.md`/`AGENTS.md`. **Fase 1.** |
| **llm-council** (comparar varios modelos) | El **handshake de revisión**: un agente critica la salida del otro antes de integrar (tu flujo "que el otro diga si está bien"). Alimenta la Fase 3 de validación. **Fase 2.** |
| **oss-contributor-swarm** | Patrón para un **modo "misión" autónomo** (varios agentes trabajando hacia un objetivo de repo). Visión a futuro del orquestador. **Fase 3+.** |

### Nivel 2 — Soporte de skills como sistema de PLUGINS (Fase 4)
El repo trae justo las herramientas para nuestro sistema de plugins de agentes.

| Skill | Uso en Nexora |
|---|---|
| **find-skills** | Descubrir skills instalables bajo demanda → buscador de capacidades en la UI de Nexora. |
| **skill-creator / happycapy-skill-creator** | Crear nuevas skills/plugins de agente desde Nexora. |
| **claude-code-templates** | Instalar plantillas/integraciones en un proyecto. |

Esto encaja con el `AgentAdapter` + el sistema de plugins del plan (añadir
Gemini/Aider/agentes propios mañana).

### Nivel 3 — Catálogo de CAPACIDADES instalables (los demás ~33 skills)
Son capacidades de usuario final, no del orquestador. Nexora las trata como un
**catálogo**: el usuario (o el orquestador) instala la skill que la tarea
necesita en el worktree del agente, y el agente la usa. No se reimplementan; se
referencian e instalan.

- **App & Web dev** (Next.js, Better Auth, Expo, Supabase, design systems, 3D web, Google Places…) → capacidades para tareas de desarrollo.
- **Diseño/Docs/Presentaciones** (canvas, slides HTML, PPTX, PDF, LaTeX, data-storytelling, writing, resume…) → capacidades de documentación/entregables.
- **Media & creative** (imagen, Gemini 3 Pro, vídeo, film, frames, GIF…) → capacidades multimedia.
- **Social & creator** (Instagram, Reddit, Xiaohongshu, cross-posting…) → capacidades de publicación.
- **Integraciones** (feishu, weather…) → conectores.

## Qué construimos (y qué NO)

- **Sí, ahora (diseño):** conceptos de Nivel 1 incorporados al diseño
  (contract-first ya; capy-cortex → memoria; llm-council → revisión).
- **Sí, en su fase:** Nivel 2 cuando lleguemos al sistema de plugins (Fase 4);
  Nivel 3 como catálogo instalable (una tarea de UI: listar/instalar skills en
  el worktree).
- **No:** forkear ni reimplementar las 45 skills dentro del MVP. Sería bloat;
  la mayoría son capacidades que el agente ya sabe cargar solo.

## Tarea derivada (futura)
**Catálogo de skills en Nexora:** UI para navegar el repo Happycapy (y otros),
instalar una skill en `<worktree>/.claude/skills/`, y que el agente la use. Es
una funcionalidad de Fase 4 (plugins), no del MVP.
