# Qué adoptamos de OpenClaw (y qué no)

> Fuente: [`openclaw/openclaw`](https://github.com/openclaw/openclaw) — asistente
> de IA personal local-first, gateway multi-canal (TS/Node). Proyecto adyacente
> a Nexora; tomamos **conceptos de arquitectura**, no el producto entero.

## Por qué NO se importa todo
OpenClaw es un *asistente personal por mensajería* (WhatsApp/Telegram/Signal…,
voz, apps móviles companion). Nexora es un *orquestador de escritorio para
agentes de código sobre un proyecto git*. Forkear OpenClaw metería 20+ canales,
voz y apps móviles que **no sirven a la misión de Nexora** = bloat que desvía.
Adoptamos lo que encaja; lo demás queda fuera (o como idea muy futura).

## Nivel 1 — Conceptos que ya validan/refuerzan el core de Nexora
| OpenClaw | En Nexora |
|---|---|
| **Gateway / control plane** de sesiones, herramientas y **eventos** | Es justo lo que ya somos: el orquestador + el bus de `AgentEvent` que acabamos de cablear (`emit("agent-event")`). Confirma la dirección. |
| **Sandboxed execution** (seguridad) | Nuestro aislamiento por **git worktree** + autonomía contenida (nada llega a main sin tu merge). Mismo principio. |
| **Skill system (ClawHub)** | Nuestro catálogo de skills/plugins (ya mapeado en [skills-integration.md](./skills-integration.md)). ClawHub es otra fuente de catálogo. |
| **Multi-agent routing** | El planificador/orquestador que enruta subtareas al agente adecuado (contract-first, Fase 2). |

## Nivel 2 — Ideas que SÍ vale la pena adoptar como features de Nexora
| OpenClaw | Adaptación en Nexora | Fase |
|---|---|---|
| **Live Canvas** (workspace visual dirigido por el agente) | Encaja con tu obsesión por "ver en tiempo real lo que hace". El stream en vivo que ya cableamos es el germen; un Canvas (preview/diff/archivos en vivo) lo lleva más lejos. | 3 (IDE feel) |
| **Notificaciones multi-canal** (un solo canal, p.ej. Telegram) | "Avísame y déjame aprobar el merge desde el móvil" cuando un agente termina o necesita al jefe. Útil, NO core. | 3+ (opcional) |
| **Companion / menú** | Un acceso de bandeja para ver estado de agentes sin abrir la app. Menor. | futuro |

## Nivel 3 — Fuera de misión (no se adopta)
Integraciones de mensajería masiva (WhatsApp/iMessage/IRC/Matrix…), wake-words y
modo voz, apps móviles como nodos. Son de un producto distinto; si algún día
Nexora quiere control remoto, basta **un** canal de notificación (Nivel 2), no
veinte.

## Resumen
Nexora ya implementa la columna de OpenClaw (control plane + eventos + sandbox).
Lo nuevo que tomamos: el concepto de **Live Canvas** (Fase 3) y, opcional, **una
notificación remota para aprobar merges**. El resto no entra: distinto producto.
