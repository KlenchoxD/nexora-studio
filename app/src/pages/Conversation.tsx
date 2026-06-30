import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import Icon from "../components/Icon";
import TermView from "../components/TermView";
import {
  detectAgents, startTask, onAgentEvent, taskDiff, cancelTask, openProject,
  listRecentTasks, openTerminal, systemStats, listDir, skillsCatalog, installSkill, readTextFile,
  readMemory, writeMemory,
  type AgentStatus, type AgentEvent, type RecentTask, type SystemStats, type DirEntry, type SkillEntry,
} from "../lib/ipc";
import "../styles/conversation.css";

type Timed = { ev: AgentEvent; at: number };
type LiveTurn = { taskId: string; agentId: string; events: Timed[]; done: boolean; diff?: string };
type PendingChain = { waitFor: string; nextAgent: string; originalPrompt: string; project: string };

const label = (id: string) => (id === "codex" ? "Codex CLI" : "Claude Code");
const baseName = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;
const fmtTime = (ms: number) =>
  new Date(ms).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
const fmtGB = (bytes: number) => (bytes / 1024 / 1024 / 1024).toFixed(1);
const fmtTokens = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`);

// --- Coordinación de equipo (modo "Ambos") ---
const TEAM_INTRO =
  "Trabajas en EQUIPO con otro agente de IA sobre esta MISMA carpeta local. " +
  "Coordínense: no rehagan el trabajo del otro, construyan sobre él. " +
  "Sé concreto y haz cambios reales en los archivos, no solo describas.";

const teamPromptClaude = (task: string) =>
  `${TEAM_INTRO}\n\nTU ROL (Claude): analiza la tarea, define el plan y ejecuta la parte de ` +
  `UI/frontend, documentación y revisión. Al terminar, deja CLARO en tu respuesta qué hiciste ` +
  `y qué le queda por implementar a Codex (backend/lógica), porque tu salida será su contexto.\n\n` +
  `TAREA:\n${task}`;

// Prefijo de contexto inyectado a TODO prompt: memoria del proyecto + modo permisos.
const ctxPrefix = (mem: string, safe: boolean) =>
  (safe ? "MODO PLAN: NO modifiques archivos ni ejecutes comandos que cambien el sistema; solo analiza y propón el plan.\n\n" : "") +
  (mem.trim() ? `MEMORIA DEL PROYECTO (decisiones y convenciones previas — respétalas):\n${mem.trim()}\n\n---\n\n` : "");

const teamPromptCodex = (task: string, claudeWork: string) =>
  `${TEAM_INTRO}\n\nTU ROL (Codex): implementa la lógica/backend y completa lo que Claude dejó ` +
  `pendiente. NO repitas lo ya hecho; continúa a partir de su trabajo.` +
  (claudeWork.trim() ? `\n\n--- Lo que Claude ya hizo/planeó ---\n${claudeWork}` : "") +
  `\n\nTAREA ORIGINAL:\n${task}`;

function agentReady(a: AgentStatus) {
  if (!a.installed) return false;
  if (a.id === "codex") return a.auth === "ok";
  return true;
}

function connInfo(a: AgentStatus): { txt: string; ok: boolean; hint?: string } {
  if (!a.installed) {
    return {
      txt: "no instalado", ok: false,
      hint: a.id === "codex" ? "npm i -g @openai/codex" : "npm i -g @anthropic-ai/claude-code",
    };
  }
  if (a.id === "codex") {
    return a.auth === "ok"
      ? { txt: "conectado", ok: true }
      : { txt: "sin login", ok: false, hint: "codex login" };
  }
  return { txt: "listo", ok: true, hint: "ejecuta  claude  una vez" };
}

const opIcon = (op?: string) =>
  op === "add" ? "✚" : op === "delete" || op === "remove" ? "✕" : "✎";

// Render mínimo de markdown inline: **negrita**, `código`. Sin librería (ponytail).
function inlineMd(s: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const re = /\*\*([^*]+)\*\*|`([^`]+)`/g;
  let last = 0; let m: RegExpExecArray | null; let i = 0;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push(s.slice(last, m.index));
    if (m[1] !== undefined) out.push(<strong key={i++}>{m[1]}</strong>);
    else if (m[2] !== undefined) out.push(<code key={i++}>{m[2]}</code>);
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push(s.slice(last));
  return out;
}
// Texto del agente → párrafos con saltos de línea y markdown inline.
function mdText(text: string): React.ReactNode {
  const lines = text.split("\n");
  return lines.map((ln, i) => (
    <span key={i}>{inlineMd(ln)}{i < lines.length - 1 ? <br /> : null}</span>
  ));
}

// Etiqueta de estado de un turno (como los badges del timeline del mockup)
function turnStatus(t: LiveTurn): { txt: string; cls: string } {
  if (t.events.some((x) => x.ev.kind === "error")) return { txt: "Error", cls: "err" };
  if (t.done) return { txt: "Completado", cls: "ok" };
  const last = t.events[t.events.length - 1]?.ev.kind;
  if (last === "tool_use") return { txt: "Ejecutando", cls: "run" };
  if (last === "file_change") return { txt: "Editando", cls: "run" };
  return { txt: "Pensando", cls: "run" };
}

function EventLine({ e }: { e: AgentEvent }) {
  switch (e.kind) {
    case "step": return <p>{mdText(e.text)}</p>;
    case "tool_use":
      if (e.name === "command" && e.detail) {
        return (
          <details className="cmd">
            <summary><span className="cmd-glyph">⚙</span> <code>{e.detail.split("\n")[0].slice(0, 100)}</code></summary>
            <pre>{e.detail}</pre>
          </details>
        );
      }
      return <div className="ev">› {e.name}</div>;
    case "file_change":
      return (
        <div className={`fc ${e.op ?? "edit"}`}>
          <span className="fc-glyph">{opIcon(e.op)}</span>
          <span className="fc-path mono">{baseName(e.path)}</span>
          {e.op && <span className="fc-op">{e.op}</span>}
        </div>
      );
    case "done": return <div className="status">✓ {e.summary ? mdText(e.summary) : "completado"}</div>;
    case "error": return <div className="err">{e.message}</div>;
    case "started": return <div className="ev">sesión iniciada{e.api_key_source ? ` · ${e.api_key_source === "none" ? "cuenta personal" : e.api_key_source}` : ""}</div>;
    case "raw": return <div className="ev" style={{ opacity: 0.5 }}>{e.json.slice(0, 120)}</div>;
    default: return null;
  }
}

// Sparkline monocromo a partir de un historial de números (0..max)
function Sparkline({ data, max }: { data: number[]; max?: number }) {
  if (data.length < 2) return <svg className="spark" viewBox="0 0 80 24" preserveAspectRatio="none" />;
  const hi = max ?? Math.max(...data, 1);
  const w = 80, h = 24;
  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * w;
    const y = h - (Math.min(v, hi) / hi) * (h - 2) - 1;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
  return (
    <svg className="spark" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none">
      <polyline points={pts} fill="none" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  );
}

// Nodo del árbol de archivos (carga perezosa al expandir)
function FileNode({ path, name, isDir, depth }: { path: string; name: string; isDir: boolean; depth: number }) {
  const [open, setOpen] = useState(false);
  const [kids, setKids] = useState<DirEntry[] | null>(null);
  const toggle = async () => {
    if (!isDir) return;
    if (kids === null) {
      try { setKids(await listDir(path)); } catch { setKids([]); }
    }
    setOpen((o) => !o);
  };
  return (
    <>
      <div className={`fnode ${isDir ? "dir" : ""}`} style={{ paddingLeft: depth * 13 + 8 }} onClick={toggle}>
        <span className="fn-caret">{isDir ? (open ? "▾" : "▸") : ""}</span>
        <Icon name={isDir ? "folder" : "file"} cls="icon sm" />
        <span className="fn-name">{name}</span>
      </div>
      {open && kids?.map((c) => (
        <FileNode key={c.name} path={`${path}\\${c.name}`} name={c.name} isDir={c.is_dir} depth={depth + 1} />
      ))}
    </>
  );
}

function SetupScreen({ agents, onVerify }: { agents: AgentStatus[]; onVerify: () => void }) {
  const [checking, setChecking] = useState(false);
  const verify = async () => { setChecking(true); await onVerify(); setChecking(false); };
  return (
    <div className="setup-screen">
      <div className="setup-card">
        <div className="elogo">N</div>
        <h1>Configura tus agentes</h1>
        <p>Nexora orquesta Codex y Claude usando <b>tus cuentas oficiales</b>.<br />
          Instala cada herramienta y autentícate una vez en la terminal.</p>
        <div className="setup-agents">
          {agents.map((a) => {
            const c = connInfo(a); const ready = agentReady(a);
            return (
              <div key={a.id} className={`setup-agent ${ready ? "ready" : ""}`}>
                <div className="sa-header">
                  <i className="d" style={{ background: ready ? "var(--running)" : "var(--idle)" }} />
                  <span className="sa-name">{a.name}</span>
                  <span className={`st ${ready ? "ok" : "off"}`}>{c.txt}</span>
                </div>
                {!ready && c.hint && (
                  <div className="sa-steps">
                    <code className="chint">{c.hint}</code>
                    {a.installed && a.id === "codex" && <code className="chint">codex login</code>}
                  </div>
                )}
                {ready && <div className="sa-ok">✓ Listo</div>}
              </div>
            );
          })}
        </div>
        <div className="setup-actions">
          <button className="btn" onClick={() => openTerminal().catch(() => {})}>
            <Icon name="terminal" cls="icon sm" /> Abrir terminal
          </button>
          <button className="btn primary" onClick={verify} disabled={checking}>
            {checking ? "Verificando…" : "↻ Verificar conexión"}
          </button>
        </div>
        <p className="connnote">
          Nexora no almacena contraseñas ni intercepta tráfico.<br />
          Solo coordina los procesos usando las sesiones que tú ya tienes abiertas.
        </p>
      </div>
    </div>
  );
}

const NEXORA_NAV = ["Agentes", "Handoffs", "Archivos", "Historial", "Configuración"];
const TABS = ["Línea de tiempo", "Archivos", "Terminal", "Registros"] as const;

export default function Conversation() {
  const [agents, setAgents] = useState<AgentStatus[]>([]);
  const [recent, setRecent] = useState<RecentTask[]>([]);
  const [project, setProject] = useState("");
  const [branch, setBranch] = useState("");
  const [prompt, setPrompt] = useState("");
  const [target, setTarget] = useState("auto");
  const [live, setLive] = useState<LiveTurn[]>([]);
  const [notice, setNotice] = useState("");
  const [tab, setTab] = useState<(typeof TABS)[number]>("Línea de tiempo");
  const [tree, setTree] = useState<DirEntry[]>([]);
  const [sys, setSys] = useState<SystemStats | null>(null);
  const [cpuHist, setCpuHist] = useState<number[]>([]);
  // Catálogo de skills (repo Happycapy)
  const [skillsOpen, setSkillsOpen] = useState(false);
  const [catalog, setCatalog] = useState<SkillEntry[]>([]);
  const [catLoading, setCatLoading] = useState(false);
  const [catErr, setCatErr] = useState("");
  const [skillQuery, setSkillQuery] = useState("");
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [installed, setInstalled] = useState<Set<string>>(new Set());
  // Live Canvas (concepto adoptado de OpenClaw): archivos tocados en vivo
  const [canvasFile, setCanvasFile] = useState<string>("");
  const [canvasContent, setCanvasContent] = useState<string>("");
  const [canvasLoading, setCanvasLoading] = useState(false);
  // Memoria compartida + política de permisos (adoptados de OpenClaw)
  const [memory, setMemory] = useState("");
  const [memSaved, setMemSaved] = useState(true);
  const [safe, setSafe] = useState(false);
  const memRef = useRef("");
  const safeRef = useRef(false);
  useEffect(() => { memRef.current = memory; }, [memory]);
  useEffect(() => { safeRef.current = safe; }, [safe]);
  const bottom = useRef<HTMLDivElement>(null);
  const chain = useRef<PendingChain | null>(null);
  const pendingLaunch = useRef<null | { agent: string; prompt: string; proj: string; desc: string }>(null);
  const chainCtx = useRef<string>("");
  const [launchTick, setLaunchTick] = useState(0);

  const refreshAgents = () => detectAgents().then(setAgents).catch(() => {});
  const refreshRecent = () => listRecentTasks().then(setRecent).catch(() => {});

  useEffect(() => { refreshAgents(); refreshRecent(); }, []);

  // Estadísticas reales del sistema (CPU/memoria) cada 2.5s
  useEffect(() => {
    let alive = true;
    const tick = () => systemStats().then((s) => {
      if (!alive) return;
      setSys(s);
      setCpuHist((h) => [...h.slice(-23), s.cpu]);
    }).catch(() => {});
    tick();
    const id = setInterval(tick, 2500);
    return () => { alive = false; clearInterval(id); };
  }, []);

  useEffect(() => {
    const un = onAgentEvent((p) => {
      const done = p.event.kind === "done" || p.event.kind === "error";
      if (chain.current?.waitFor === p.task_id) {
        if (p.event.kind === "step") chainCtx.current += p.event.text + "\n";
        if (done) {
          const { nextAgent, originalPrompt, project: proj } = chain.current;
          const ctx = chainCtx.current;
          chain.current = null;
          chainCtx.current = "";
          pendingLaunch.current = {
            agent: nextAgent, prompt: ctxPrefix(memRef.current, safeRef.current) + teamPromptCodex(originalPrompt, ctx), proj, desc: originalPrompt,
          };
          setLaunchTick((n) => n + 1);
        }
      }
      setLive((prev) => {
        const i = prev.findIndex((t) => t.taskId === p.task_id);
        const next = [...prev];
        const timed: Timed = { ev: p.event, at: Date.now() };
        if (i === -1) {
          next.push({ taskId: p.task_id, agentId: p.agent_id, events: [timed], done });
        } else {
          next[i] = { ...next[i], events: [...next[i].events, timed], done: next[i].done || done };
        }
        return next;
      });
      if (done) refreshRecent();
    });
    return () => { un.then((f) => f()); };
  }, []);

  useEffect(() => {
    if (!pendingLaunch.current) return;
    const { agent, prompt: p, proj, desc } = pendingLaunch.current;
    pendingLaunch.current = null;
    startTask(agent, p, proj, desc, safeRef.current).then((taskId) => {
      setLive((s) => [...s, { taskId, agentId: agent, events: [], done: false }]);
    }).catch((e) => {
      setLive((s) => [...s, {
        taskId: `err-${Date.now()}-${agent}`, agentId: agent,
        events: [{ ev: { kind: "error", message: String(e) }, at: Date.now() }], done: true,
      }]);
    });
  }, [launchTick]);

  useEffect(() => { bottom.current?.scrollIntoView({ behavior: "smooth" }); }, [live]);

  // Estadísticas por agente derivadas de los eventos en vivo (datos reales)
  const agentStats = useMemo(() => {
    const m: Record<string, { model?: string; tin: number; tout: number; cost: number; active: boolean }> = {};
    for (const t of live) {
      const a = (m[t.agentId] ??= { tin: 0, tout: 0, cost: 0, active: false });
      if (!t.done) a.active = true;
      for (const { ev } of t.events) {
        if (ev.kind === "started" && ev.model) a.model = ev.model;
        if (ev.kind === "token_usage") { a.tin += ev.input; a.tout += ev.output; }
        if (ev.kind === "done" && ev.cost_usd) a.cost += ev.cost_usd;
      }
    }
    return m;
  }, [live]);

  // Archivos tocados en la sesión (de los eventos file_change), último por ruta
  const changedFiles = useMemo(() => {
    const m = new Map<string, { path: string; op?: string; agent: string; at: number }>();
    for (const t of live) {
      for (const { ev, at } of t.events) {
        if (ev.kind === "file_change") m.set(ev.path, { path: ev.path, op: ev.op, agent: t.agentId, at });
      }
    }
    return [...m.values()].sort((a, b) => b.at - a.at);
  }, [live]);

  const absPath = (p: string) => (/^([a-zA-Z]:[\\/]|[\\/])/.test(p) ? p : `${project}\\${p}`);

  const openInCanvas = async (path: string) => {
    setCanvasFile(path); setCanvasLoading(true); setCanvasContent("");
    try { setCanvasContent(await readTextFile(absPath(path))); }
    catch (e) { setCanvasContent(`// ${e}`); }
    finally { setCanvasLoading(false); }
  };

  const totalTokens = Object.values(agentStats).reduce((s, a) => s + a.tin + a.tout, 0);
  const totalCost = Object.values(agentStats).reduce((s, a) => s + a.cost, 0);
  const lastChainTurn = [...live].reverse().find((t) => t.agentId === "codex");

  const anyReady = agents.some(agentReady);
  const targetReady = (() => {
    if (target === "both") return agents.filter(agentReady).length >= 1;
    const id = target === "auto" ? "claude" : target;
    const a = agents.find((x) => x.id === id);
    return a ? agentReady(a) : false;
  })();

  const loadTree = async (dir: string) => {
    try { setTree(await listDir(dir)); } catch { setTree([]); }
  };

  const pickFolder = async () => {
    const dir = await open({ directory: true, title: "Elige la carpeta del proyecto" });
    if (typeof dir !== "string") return;
    setProject(dir);
    setNotice("");
    loadTree(dir);
    readMemory(dir).then((m) => { setMemory(m); setMemSaved(true); }).catch(() => setMemory(""));
    try { setBranch(await openProject(dir)); }
    catch (e) { setBranch(""); setNotice(String(e)); }
  };

  const saveMemory = () => {
    if (!project) return;
    writeMemory(project, memory).then(() => setMemSaved(true)).catch((e) => setNotice(String(e)));
  };

  const send = async () => {
    if (!prompt.trim()) return;
    if (!project) { setNotice("Abre una carpeta para enviar tareas."); return; }
    if (!targetReady) { setNotice("El agente seleccionado no está listo."); return; }
    const text = prompt;
    setPrompt(""); setNotice(""); setTab("Línea de tiempo");
    const pfx = ctxPrefix(memory, safe); // memoria + modo permisos

    if (target === "both") {
      const readyIds = ["claude", "codex"].filter((id) => {
        const a = agents.find((x) => x.id === id); return a ? agentReady(a) : false;
      });
      if (readyIds.length === 0) return;
      if (readyIds.length === 1) {
        try {
          const taskId = await startTask(readyIds[0], pfx + text, project, text, safe);
          setLive((prev) => [...prev, { taskId, agentId: readyIds[0], events: [], done: false }]);
        } catch (e) {
          setLive((prev) => [...prev, { taskId: `err-${Date.now()}`, agentId: readyIds[0], events: [{ ev: { kind: "error", message: String(e) }, at: Date.now() }], done: true }]);
        }
      } else {
        try {
          const taskId = await startTask("claude", pfx + teamPromptClaude(text), project, text, safe);
          chain.current = { waitFor: taskId, nextAgent: "codex", originalPrompt: text, project };
          setLive((prev) => [...prev, { taskId, agentId: "claude", events: [], done: false }]);
        } catch (e) {
          setLive((prev) => [...prev, { taskId: `err-${Date.now()}`, agentId: "claude", events: [{ ev: { kind: "error", message: String(e) }, at: Date.now() }], done: true }]);
        }
      }
    } else {
      const a = target === "auto" ? "claude" : target;
      try {
        const taskId = await startTask(a, pfx + text, project, text, safe);
        setLive((prev) => [...prev, { taskId, agentId: a, events: [], done: false }]);
      } catch (e) {
        setLive((prev) => [...prev, { taskId: `err-${Date.now()}-${a}`, agentId: a, events: [{ ev: { kind: "error", message: String(e) }, at: Date.now() }], done: true }]);
      }
    }
    refreshRecent();
  };

  const patch = (id: string, p: Partial<LiveTurn>) =>
    setLive((prev) => prev.map((t) => (t.taskId === id ? { ...t, ...p } : t)));

  const cancel = async (id: string) => {
    try { await cancelTask(id); } catch { /* noop */ }
    setLive((prev) => prev.map((t) => (t.taskId === id
      ? { ...t, done: true, events: [...t.events, { ev: { kind: "error", message: "cancelado" } as AgentEvent, at: Date.now() }] } : t)));
    refreshRecent();
  };

  const viewDiff = async (id: string) => {
    try { patch(id, { diff: (await taskDiff(project)) || "(sin cambios)" }); }
    catch (e) { patch(id, { diff: String(e) }); }
  };

  const openSkills = async () => {
    setSkillsOpen(true);
    // qué skills ya están instaladas en la carpeta actual
    if (project) {
      listDir(`${project}\\.claude\\skills`)
        .then((es) => setInstalled(new Set(es.filter((e) => e.is_dir).map((e) => e.name))))
        .catch(() => setInstalled(new Set()));
    }
    if (catalog.length > 0) return;
    setCatLoading(true); setCatErr("");
    try { setCatalog(await skillsCatalog()); }
    catch (e) { setCatErr(String(e)); }
    finally { setCatLoading(false); }
  };

  const doInstall = async (s: SkillEntry) => {
    if (!project) { setCatErr("Abre una carpeta antes de instalar skills."); return; }
    setInstalling((p) => new Set(p).add(s.name));
    try {
      await installSkill(project, s.name, s.files);
      setInstalled((p) => new Set(p).add(s.name));
    } catch (e) {
      setCatErr(`No se pudo instalar ${s.name}: ${e}`);
    } finally {
      setInstalling((p) => { const n = new Set(p); n.delete(s.name); return n; });
    }
  };

  const filteredSkills = catalog.filter((s) => s.name.toLowerCase().includes(skillQuery.toLowerCase()));

  if (agents.length > 0 && !anyReady) {
    return <div className="ide ide-setup"><SetupScreen agents={agents} onVerify={refreshAgents} /></div>;
  }

  return (
    <div className="ide">
      {/* ---------- Columna izquierda: explorador ---------- */}
      <aside className="explorer">
        <button className="proj-head" onClick={pickFolder} title="Cambiar carpeta">
          <Icon name="folderOpen" cls="icon sm" />
          <span className="ph-name">{project ? baseName(project) : "Abrir carpeta"}</span>
          {branch && <span className="ph-branch mono">{branch}</span>}
        </button>

        <div className="ex-section">
          <div className="label">Explorador</div>
          <div className="ftree">
            {!project && <div className="ex-empty">Abre una carpeta para ver sus archivos.</div>}
            {project && tree.length === 0 && <div className="ex-empty">(vacío)</div>}
            {tree.map((c) => (
              <FileNode key={c.name} path={`${project}\\${c.name}`} name={c.name} isDir={c.is_dir} depth={0} />
            ))}
          </div>
        </div>

        <div className="ex-section">
          <div className="label">Nexora</div>
          <nav className="ex-nav">
            <button className="ex-navitem" onClick={openSkills}>
              <Icon name="sparkle" cls="icon sm" /> Skills
            </button>
            {NEXORA_NAV.map((n) => (
              <button key={n} className="ex-navitem" disabled title="Próximamente">
                <span className="dot idle" /> {n}
              </button>
            ))}
          </nav>
        </div>

        <div className="ex-foot">
          <span className="dot run" /> Sincronizado
        </div>
      </aside>

      {/* ---------- Columna central: timeline + composer ---------- */}
      <section className="center">
        <div className="tabs">
          {TABS.map((t) => (
            <button
              key={t}
              className={`tab ${tab === t ? "active" : ""}`}
              onClick={() => setTab(t)}
            >
              {t}
              {t === "Archivos" && changedFiles.length > 0 && <span className="tab-count">{changedFiles.length}</span>}
            </button>
          ))}
          <span className="spacer" />
          {agents.some((a) => !agentReady(a)) && (
            <button className="btn sm" onClick={refreshAgents}>↻ Verificar</button>
          )}
        </div>

        <div className="timeline-wrap">
          {tab === "Archivos" ? (
            <div className="canvas">
              <div className="canvas-list">
                <div className="label" style={{ padding: "0 4px 8px" }}>Cambios en vivo · {changedFiles.length}</div>
                {changedFiles.length === 0 && <div className="ex-empty">Aún no hay archivos tocados. Cuando un agente cree o edite algo, aparecerá aquí en vivo.</div>}
                {changedFiles.map((f) => (
                  <button key={f.path} className={`canvas-item ${canvasFile === f.path ? "active" : ""}`} onClick={() => openInCanvas(f.path)}>
                    <span className={`fc-glyph ${f.op ?? "edit"}`}>{opIcon(f.op)}</span>
                    <span className="ci-name mono">{baseName(f.path)}</span>
                    <span className="ci-agent">{f.agent === "codex" ? "Codex" : "Claude"}</span>
                  </button>
                ))}
              </div>
              <div className="canvas-view">
                {!canvasFile ? (
                  <div className="ex-empty" style={{ padding: 24 }}>Selecciona un archivo para ver su contenido actual.</div>
                ) : (
                  <>
                    <div className="cv-head"><span className="mono">{canvasFile}</span></div>
                    <pre className="cv-body">{canvasLoading ? "cargando…" : canvasContent}</pre>
                  </>
                )}
              </div>
            </div>
          ) : tab === "Terminal" ? (
            <TermView cwd={project} />
          ) : tab !== "Línea de tiempo" ? (
            <div className="tab-placeholder">Los eventos crudos se muestran en la línea de tiempo.</div>
          ) : live.length === 0 ? (
            <div className="empty">
              <div className="elogo">N</div>
              <h1>Orquesta Codex y Claude</h1>
              <p>Trabajan directo sobre tu carpeta local. Tú revisas los cambios cuando terminan.</p>
              {!project && (
                <button className="btn primary" onClick={pickFolder}>
                  <Icon name="folderOpen" cls="icon sm" /> Abrir carpeta
                </button>
              )}
              {project && <p className="ehint">Carpeta <b className="mono">{baseName(project)}</b> — escribe tu primera tarea abajo.</p>}
            </div>
          ) : (
            <div className="timeline">
              {live.map((t) => {
                const st = turnStatus(t);
                const t0 = t.events[0]?.at ?? Date.now();
                return (
                  <article className="tl-turn" key={t.taskId}>
                    <div className="tl-gutter">
                      <span className="tl-time">{fmtTime(t0)}</span>
                      <span className={`tl-node ${st.cls}`} />
                    </div>
                    <div className="tl-body">
                      <div className="tl-head">
                        <span className={`av ${t.agentId === "codex" ? "cx" : "cl"}`}>{t.agentId === "codex" ? "C" : "A"}</span>
                        <span className="nm">{label(t.agentId)}</span>
                        <span className={`badge ${st.cls}`}>{st.txt}</span>
                        {!t.done && <button className="btn sm" onClick={() => cancel(t.taskId)}>Cancelar</button>}
                      </div>
                      <div className="content">
                        {t.events.length === 0 && <div className="ev">lanzando agente…</div>}
                        {t.events.map(({ ev }, i) => <EventLine e={ev} key={i} />)}
                        {t.done && !t.taskId.startsWith("err") && !t.events.some((e) => e.ev.kind === "error") && !t.diff && (
                          <div className="acts"><button className="btn sm" onClick={() => viewDiff(t.taskId)}>Ver cambios</button></div>
                        )}
                        {t.diff && (
                          <figure className="code">
                            <figcaption><span className="mono">cambios en tu carpeta</span></figcaption>
                            <pre>{t.diff}</pre>
                          </figure>
                        )}
                      </div>
                    </div>
                  </article>
                );
              })}
              <div ref={bottom} />
            </div>
          )}
        </div>

        <div className="composer">
          {notice && <div className="notice">{notice}</div>}
          <div className="box">
            <textarea
              rows={1}
              placeholder={!project ? "Abre una carpeta para empezar…"
                : !targetReady ? "El agente seleccionado no está listo →"
                : "Describe la tarea que quieres que realicen los agentes…"}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }}
            />
            <div className="cbar">
              <div className="to">
                {(["auto", "codex", "claude", "both"] as const).map((tg) => {
                  const agId = tg === "auto" ? "claude" : tg;
                  const ag = agents.find((a) => a.id === agId);
                  const ready = tg === "both" ? agents.some(agentReady) : ag ? agentReady(ag) : false;
                  return (
                    <button key={tg} className={`seg ${target === tg ? "active" : ""} ${!ready ? "seg-off" : ""}`} onClick={() => setTarget(tg)}>
                      {tg === "auto" ? "Auto" : tg === "both" ? "Ambos" : tg === "codex" ? "Codex" : "Claude"}
                      {!ready && <span className="seg-dot" />}
                    </button>
                  );
                })}
              </div>
              <button
                className={`seg safe-toggle ${safe ? "active" : ""}`}
                onClick={() => setSafe((s) => !s)}
                title="Modo plan: los agentes analizan y proponen sin modificar archivos"
              >
                {safe ? "● Plan" : "Plan"}
              </button>
              <span style={{ flex: 1 }} />
              <button className="send" aria-label="Enviar" onClick={send} disabled={!project || !targetReady}
                style={{ opacity: (!project || !targetReady) ? 0.35 : 1 }}>
                <Icon name="arrowRight" cls="icon sm" />
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* ---------- Columna derecha: cards ---------- */}
      <aside className="rightbar">
        <div className="rcard">
          <div className="rc-head"><span className="label">Agentes</span><button className="iconbtn sm" onClick={refreshAgents} title="Re-detectar">↻</button></div>
          {agents.length === 0 && <div className="ex-empty">detectando…</div>}
          {agents.map((a) => {
            const c = connInfo(a); const ready = agentReady(a);
            const s = agentStats[a.id];
            return (
              <div className="agent-row" key={a.id}>
                <div className="ar-top">
                  <span className={`av ${a.id === "codex" ? "cx" : "cl"}`}>{a.id === "codex" ? "C" : "A"}</span>
                  <span className="ar-name">{a.name}</span>
                  <span className={`badge ${ready ? (s?.active ? "run" : "ok") : "off"}`}>
                    {ready ? (s?.active ? "Activo" : "Listo") : c.txt}
                  </span>
                </div>
                <div className="ar-model">{s?.model ? `Modelo: ${s.model}` : (a.id === "codex" ? "Codex CLI · cuenta ChatGPT" : "Claude · cuenta personal")}</div>
                <div className="ar-metrics">
                  <span className="metric">{fmtTokens((s?.tin ?? 0) + (s?.tout ?? 0))} tokens</span>
                  {s && s.cost > 0 && <span className="metric">${s.cost.toFixed(3)}</span>}
                  {!ready && c.hint && <code className="chint">{c.hint}</code>}
                </div>
              </div>
            );
          })}
        </div>

        <div className="rcard">
          <div className="rc-head"><span className="label">Cola de tareas</span></div>
          <div className="queue">
            {recent.length === 0 && <div className="ex-empty">sin tareas todavía</div>}
            {recent.slice(0, 6).map((r) => (
              <div className={`qitem ${r.status === "running" ? "active" : ""}`} key={r.id}>
                <span className={`qstate ${r.status}`} />
                <span className="qtext">{r.description}</span>
                <span className="qagent">{r.agent_id === "codex" ? "Codex" : "Claude"}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="rcard">
          <div className="rc-head"><span className="label">Estado del sistema</span></div>
          <div className="sysrow">
            <span className="sl">CPU</span>
            <span className="sv num">{sys ? `${sys.cpu.toFixed(0)}%` : "—"}</span>
            <span className="sparkwrap"><Sparkline data={cpuHist} max={100} /></span>
          </div>
          <div className="sysrow">
            <span className="sl">Memoria</span>
            <span className="sv num">{sys ? `${fmtGB(sys.mem_used)} / ${fmtGB(sys.mem_total)} GB` : "—"}</span>
          </div>
          <div className="sysrow">
            <span className="sl">Tokens (sesión)</span>
            <span className="sv num">{fmtTokens(totalTokens)}</span>
          </div>
          <div className="sysrow">
            <span className="sl">Costo (Claude)</span>
            <span className="sv num">${totalCost.toFixed(3)}</span>
          </div>
        </div>

        <div className="rcard">
          <div className="rc-head"><span className="label">Handoff actual</span></div>
          {chain.current || lastChainTurn ? (
            <>
              <div className="handoff-flow">
                <span className="pill">Claude Code</span>
                <Icon name="arrowRight" cls="icon sm" />
                <span className="pill">Codex CLI</span>
              </div>
              <div className="handoff-meta">
                Razón: implementación técnica<br />
                Estado: {chain.current ? "esperando a Claude…" : lastChainTurn?.done ? "completado" : "en curso"}
              </div>
            </>
          ) : (
            <div className="ex-empty">Sin handoff activo. Usa el modo <b>Ambos</b> para que Claude pase el trabajo a Codex.</div>
          )}
        </div>

        <div className="rcard">
          <div className="rc-head">
            <span className="label">Memoria del proyecto</span>
            {!memSaved && <button className="btn sm primary" onClick={saveMemory}>Guardar</button>}
          </div>
          <p className="mem-note">Decisiones y convenciones que se inyectan a Claude y Codex en cada tarea.</p>
          <textarea
            className="mem-area"
            placeholder={project ? "Ej: usar TypeScript estricto. La API vive en /api. No tocar legacy/." : "Abre una carpeta para usar la memoria."}
            value={memory}
            disabled={!project}
            onChange={(e) => { setMemory(e.target.value); setMemSaved(false); }}
            onBlur={() => { if (!memSaved) saveMemory(); }}
          />
        </div>
      </aside>

      {/* ---------- Modal: catálogo de skills ---------- */}
      {skillsOpen && (
        <div className="modal-backdrop" onClick={() => setSkillsOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <div>
                <h2>Catálogo de skills</h2>
                <p className="modal-sub">
                  Se instalan en <code>{project ? `${baseName(project)}\\.claude\\skills` : ".claude/skills"}</code> y Claude las carga sola.
                  Fuente: happycapy-ai/Happycapy-skills (MIT).
                </p>
              </div>
              <button className="iconbtn" onClick={() => setSkillsOpen(false)} aria-label="Cerrar"><Icon name="x" cls="icon sm" /></button>
            </div>

            {!project && <div className="modal-warn">Abre una carpeta para poder instalar skills en ella.</div>}
            {catErr && <div className="modal-warn">{catErr}</div>}

            <input
              className="skill-search"
              placeholder="Buscar skill…"
              value={skillQuery}
              onChange={(e) => setSkillQuery(e.target.value)}
            />

            <div className="skill-list">
              {catLoading && <div className="ex-empty">Cargando catálogo desde GitHub…</div>}
              {!catLoading && filteredSkills.length === 0 && <div className="ex-empty">Sin resultados.</div>}
              {filteredSkills.map((s) => {
                const isInst = installed.has(s.name);
                const isBusy = installing.has(s.name);
                return (
                  <div className="skill-row" key={s.name}>
                    <div className="sk-info">
                      <span className="sk-name mono">{s.name}</span>
                      <span className="sk-files">{s.files.length} archivo{s.files.length === 1 ? "" : "s"}</span>
                    </div>
                    <button
                      className={`btn sm ${isInst ? "" : "primary"}`}
                      disabled={!project || isBusy || isInst}
                      onClick={() => doInstall(s)}
                    >
                      {isInst ? "✓ Instalada" : isBusy ? "Instalando…" : "Instalar"}
                    </button>
                  </div>
                );
              })}
            </div>

            <div className="modal-foot">
              {catalog.length > 0 && <span>{catalog.length} skills disponibles</span>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
