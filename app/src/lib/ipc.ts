import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AgentStatus = { id: string; name: string; installed: boolean; auth: string };

export type AgentEvent =
  | { kind: "started"; session_id?: string; model?: string; api_key_source?: string }
  | { kind: "step"; text: string }
  | { kind: "tool_use"; name: string; detail?: string }
  | { kind: "file_change"; path: string; op?: string }
  | { kind: "token_usage"; input: number; output: number; cost_usd?: number }
  | { kind: "done"; success: boolean; summary?: string; cost_usd?: number }
  | { kind: "error"; message: string }
  | { kind: "raw"; json: string };

export type EventPayload = { task_id: string; agent_id: string; event: AgentEvent };

export const detectAgents = () => invoke<AgentStatus[]>("detect_agents");

// description: texto que se muestra en "Tareas recientes" (intención del usuario).
// prompt: lo que recibe el agente (puede llevar el preámbulo de equipo). Si no se
// pasa description, se usa el prompt como antes.
export const startTask = (agentId: string, prompt: string, projectPath: string, description?: string) =>
  invoke<string>("start_task", { agentId, prompt, projectPath, description });

export const onAgentEvent = (cb: (p: EventPayload) => void): Promise<UnlistenFn> =>
  listen<EventPayload>("agent-event", (e) => cb(e.payload));

export const taskDiff = (projectPath: string) =>
  invoke<string>("task_diff", { projectPath });

export const cancelTask = (taskId: string) => invoke<void>("cancel_task", { taskId });

export const openProject = (path: string) => invoke<string>("open_project", { path });

export type RecentTask = { id: string; agent_id?: string; description: string; status: string; cost_usd?: number };
export const listRecentTasks = () => invoke<RecentTask[]>("list_recent_tasks");

export const openTerminal = () => invoke<void>("open_terminal");

export type SystemStats = { cpu: number; mem_used: number; mem_total: number };
export const systemStats = () => invoke<SystemStats>("system_stats");

export type DirEntry = { name: string; is_dir: boolean };
export const listDir = (path: string) => invoke<DirEntry[]>("list_dir", { path });

export const readTextFile = (path: string) => invoke<string>("read_text_file", { path });

export type SkillEntry = { name: string; files: string[] };
export const skillsCatalog = () => invoke<SkillEntry[]>("skills_catalog");
export const installSkill = (projectPath: string, name: string, files: string[]) =>
  invoke<number>("install_skill", { projectPath, name, files });
