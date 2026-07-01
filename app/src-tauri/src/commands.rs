//! Comandos Tauri: puente entre la UI y el backend. Persisten en SQLite y
//! transmiten eventos en vivo. Sin tocar credenciales (principio I).

use crate::agents::{claude::Claude, codex::Codex, AgentAdapter};
use crate::events::AgentEvent;
use crate::{db, runner, worktree, AppState};
use serde::Serialize;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, State};

// ---------- US1: detección ----------

#[derive(Serialize, Clone)]
pub struct AgentStatus {
    id: String,
    name: String,
    installed: bool,
    auth: String, // "ok" | "logged_out" | "unknown"
}

/// En Windows los CLIs de npm son wrappers .cmd; Tauri no hereda el PATH
/// completo de PowerShell, así que pasamos por `cmd /c` para encontrarlos.
fn cli_installed(program: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd")
            .args(["/c", program, "--version"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: sin ventana de consola
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn codex_auth() -> String {
    // Codex guarda las credenciales en ~/.codex/auth.json tras `codex login`.
    // Comprobar el archivo es más fiable que ejecutar un subcomando de estado.
    let auth_file = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| std::path::PathBuf::from(h).join(".codex").join("auth.json"));
    if auth_file.as_ref().map(|p| p.exists()).unwrap_or(false) {
        return "ok".into();
    }
    // Fallback: intentar el subcomando de estado
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "codex", "whoami"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
    if ok { "ok".into() } else { "logged_out".into() }
}

#[tauri::command]
pub fn detect_agents() -> Vec<AgentStatus> {
    let codex_in = cli_installed("codex");
    let claude_in = cli_installed("claude");
    vec![
        AgentStatus {
            id: "codex".into(),
            name: "Codex CLI".into(),
            installed: codex_in,
            auth: if codex_in { codex_auth() } else { "logged_out".into() },
        },
        AgentStatus {
            id: "claude".into(),
            name: "Claude Code".into(),
            installed: claude_in,
            auth: if claude_in { "unknown".into() } else { "logged_out".into() },
        },
    ]
}

// ---------- proyecto ----------

#[tauri::command]
pub fn open_project(state: State<AppState>, path: String) -> Result<String, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("'{path}' no existe o no es una carpeta"));
    }
    // git es OPCIONAL: si la carpeta es un repo, mostramos la rama y podremos
    // ofrecer "ver cambios"; si no, igual trabajamos directo sobre los archivos.
    let branch = if worktree::is_git_repo(&dir) {
        worktree::current_branch(&dir).unwrap_or_else(|_| "main".into())
    } else {
        "sin git".into()
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::upsert_project(&conn, &path, &path, &branch).map_err(|e| e.to_string())?;
    Ok(branch)
}

// ---------- US2: lanzar tarea + stream en vivo ----------

#[derive(Serialize, Clone)]
pub struct EventPayload {
    task_id: String,
    agent_id: String,
    event: AgentEvent,
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

#[tauri::command]
pub fn start_task(
    app: AppHandle,
    state: State<AppState>,
    agent_id: String,
    prompt: String,
    project_path: String,
    description: Option<String>,
    safe: Option<bool>,
) -> Result<String, String> {
    let dir = PathBuf::from(&project_path);
    if !dir.is_dir() {
        return Err(format!("'{project_path}' no es una carpeta"));
    }

    let task_id = short_id();
    // Lo que se muestra en "Tareas recientes": la intención del usuario, no el
    // prompt con el preámbulo de coordinación. Si no llega, se usa el prompt.
    let desc = description.as_deref().unwrap_or(&prompt);

    // persistencia inicial
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = db::upsert_project(&conn, &project_path, &project_path, "local");
        db::insert_task(&conn, &task_id, Some(&project_path), &agent_id, desc, "running")
            .map_err(|e| e.to_string())?;
    }

    let adapter: Box<dyn AgentAdapter + Send> =
        if agent_id == "codex" { Box::new(Codex) } else { Box::new(Claude) };
    // El agente trabaja DIRECTO en tu carpeta (como Claude Code). git, si la
    // carpeta lo tiene, es la red de seguridad para revisar/deshacer; no se exige.
    let mut cmd = adapter.build_command(&prompt, &dir, safe.unwrap_or(false));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn falló: {e}"))?;
    // El prompt se envía por STDIN (evita que cmd.exe corte prompts multilínea en
    // Windows). Se escribe en un hilo aparte para no deadlockear con el stdout del
    // hijo, y al soltar el pipe se cierra (EOF) para que el agente sepa que terminó.
    if let Some(mut si) = child.stdin.take() {
        use std::io::Write;
        let p = prompt.clone();
        std::thread::spawn(move || {
            let _ = si.write_all(p.as_bytes());
        });
    }
    let stdout = child.stdout.take().ok_or("sin stdout")?;
    let stderr = child.stderr.take().ok_or("sin stderr")?;

    // Leer stderr en un hilo aparte para (a) no deadlockear si el buffer se llena
    // y (b) poder mostrar el error real del agente si falla sin escribir en stdout.
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    {
        use std::io::Read;
        let err_buf = err_buf.clone();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut s);
            if let Ok(mut b) = err_buf.lock() {
                *b = s;
            }
        });
    }

    // registrar el hijo para poder cancelarlo
    state
        .jobs
        .lock()
        .map_err(|e| e.to_string())?
        .insert(task_id.clone(), child);

    let db = state.db.clone();
    let jobs = state.jobs.clone();
    let tid = task_id.clone();
    let aid = agent_id.clone();

    std::thread::spawn(move || {
        let mut final_cost: Option<f64> = None;
        let mut errored: Option<String> = None;
        let mut saw_done = false;
        let mut saw_content = false;

        let _ = runner::pump(
            BufReader::new(stdout),
            |l| adapter.parse_line(l),
            |ev| {
                if let Ok(conn) = db.lock() {
                    let payload = serde_json::to_string(&ev).unwrap_or_default();
                    let _ = db::insert_event(&conn, &tid, ev.kind_str(), &payload);
                }
                match &ev {
                    AgentEvent::Done { cost_usd, .. } => {
                        saw_done = true;
                        final_cost = *cost_usd;
                    }
                    AgentEvent::Error { message } => errored = Some(message.clone()),
                    AgentEvent::Step { .. } | AgentEvent::ToolUse { .. } => saw_content = true,
                    _ => {}
                }
                let _ = app.emit(
                    "agent-event",
                    EventPayload { task_id: tid.clone(), agent_id: aid.clone(), event: ev },
                );
            },
        );

        // emite un evento a la UI y lo persiste
        let emit = |ev: AgentEvent| {
            if let Ok(conn) = db.lock() {
                let payload = serde_json::to_string(&ev).unwrap_or_default();
                let _ = db::insert_event(&conn, &tid, ev.kind_str(), &payload);
            }
            let _ = app.emit(
                "agent-event",
                EventPayload { task_id: tid.clone(), agent_id: aid.clone(), event: ev },
            );
        };

        let cancelled = jobs.lock().ok().map(|m| !m.contains_key(&tid)).unwrap_or(false);
        let raw_stderr = err_buf.lock().ok().map(|b| b.clone()).unwrap_or_default();
        // Filtrar avisos benignos (no son errores): trust dialog, permissions.allow, etc.
        let stderr_txt = raw_stderr
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty()
                    && !t.contains("permissions.allow")
                    && !t.contains("has not been trusted")
                    && !t.contains("hasTrustDialogAccepted")
                    && !t.contains("Run Claude Code interactively")
                    && !t.contains("set projects[")
            })
            .collect::<Vec<_>>()
            .join("\n");

        if !saw_done && errored.is_none() && !cancelled {
            if !saw_content && !stderr_txt.trim().is_empty() {
                // El proceso no produjo nada útil pero escribió en stderr: error real
                // del agente (p.ej. login caducado, flag inválido). Mostrarlo, no ocultarlo.
                let tail = stderr_txt.trim().lines().rev().take(10)
                    .collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                errored = Some(tail.clone());
                emit(AgentEvent::Error { message: tail });
            } else {
                // Codex (codex exec) cierra el stream tras `turn.completed` SIN emitir
                // `thread.completed`; sin Done el turno se queda "trabajando". Lo
                // sintetizamos para cerrar el turno en la UI.
                emit(AgentEvent::Done { success: true, summary: None, cost_usd: final_cost });
                saw_done = true;
            }
        }

        // finalizar: no pisar un estado 'cancelled' puesto por cancel_task
        if let Ok(conn) = db.lock() {
            if let Some(err) = &errored {
                let _ = db::set_task_status(&conn, &tid, "failed", final_cost, Some(err));
            } else if saw_done {
                let _ = db::set_task_status(&conn, &tid, "done", final_cost, None);
            }
            // ponytail: si el stream acaba sin Done ni Error (matado/caído) se
            // deja el estado tal cual (cancel_task ya habrá puesto 'cancelled').
        }
        if let Some(mut c) = jobs.lock().ok().and_then(|mut m| m.remove(&tid)) {
            let _ = c.wait();
        }
    });

    Ok(task_id)
}

#[tauri::command]
pub fn cancel_task(state: State<AppState>, task_id: String) -> Result<(), String> {
    if let Some(mut child) = state.jobs.lock().map_err(|e| e.to_string())?.remove(&task_id) {
        let _ = child.kill();
    }
    if let Ok(conn) = state.db.lock() {
        let _ = db::set_task_status(&conn, &task_id, "cancelled", None, None);
    }
    Ok(())
}

#[tauri::command]
pub fn list_recent_tasks(state: State<AppState>) -> Result<Vec<db::TaskRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::recent_tasks(&conn, 20).map_err(|e| e.to_string())
}

// ---------- US3b: abrir terminal ----------

#[tauri::command]
pub fn open_terminal() -> Result<(), String> {
    // Windows Terminal > PowerShell > cmd
    if Command::new("wt").spawn().is_ok() { return Ok(()); }
    Command::new("cmd")
        .args(["/c", "start", "powershell"])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------- US4: revisión de cambios ----------

/// Muestra qué cambió en la carpeta. Si es repo git, el diff real; si no,
/// un aviso de que los cambios ya están aplicados en los archivos.
#[tauri::command]
pub fn task_diff(project_path: String) -> Result<String, String> {
    let dir = PathBuf::from(&project_path);
    if worktree::is_git_repo(&dir) {
        let d = worktree::diff(&dir)?;
        Ok(if d.trim().is_empty() {
            "(sin cambios respecto al último commit)".into()
        } else {
            d
        })
    } else {
        Ok("Esta carpeta no usa git: los cambios del agente ya están aplicados \
            en tus archivos. Haz \"git init\" si quieres ver diffs y poder deshacer."
            .into())
    }
}

// ---------- Datos reales del sistema (CPU/memoria) ----------

#[derive(Serialize, Clone)]
pub struct SystemStats {
    pub cpu: f32,        // % uso global
    pub mem_used: u64,   // bytes
    pub mem_total: u64,  // bytes
}

#[tauri::command]
pub fn system_stats() -> SystemStats {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    // El % de CPU necesita dos muestras con un intervalo mínimo entre ellas.
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    SystemStats {
        cpu: sys.global_cpu_usage(),
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
    }
}

// ---------- Explorador de archivos (un nivel, carga perezosa) ----------

#[derive(Serialize, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

#[tauri::command]
pub fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out: Vec<DirEntry> = Vec::new();
    for e in std::fs::read_dir(&path).map_err(|e| e.to_string())? {
        let e = match e { Ok(e) => e, Err(_) => continue };
        let name = e.file_name().to_string_lossy().into_owned();
        // saltar ruido pesado que no aporta al árbol
        if name == "node_modules" || name == ".git" || name == "target" || name == "dist" {
            continue;
        }
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        out.push(DirEntry { name, is_dir });
    }
    // carpetas primero, luego alfabético (case-insensitive)
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

// ---------- Memoria compartida del proyecto (concepto memory-host de OpenClaw) ----------
// Decisiones/convenciones que persisten entre sesiones y se inyectan a ambos
// agentes para que no se repitan ni se contradigan. Vive en .nexora/memory.md.

fn memory_path(project: &str) -> PathBuf {
    PathBuf::from(project).join(".nexora").join("memory.md")
}

#[tauri::command]
pub fn read_memory(project_path: String) -> Result<String, String> {
    Ok(std::fs::read_to_string(memory_path(&project_path)).unwrap_or_default())
}

#[tauri::command]
pub fn write_memory(project_path: String, content: String) -> Result<(), String> {
    let p = memory_path(&project_path);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| e.to_string())
}

// ---------- Live Canvas: leer contenido de un archivo tocado por un agente ----------

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 400_000 {
        return Err(format!("archivo grande ({} KB) — ábrelo en tu editor", meta.len() / 1024));
    }
    std::fs::read_to_string(&path)
        .map_err(|_| "no es un archivo de texto (o no se pudo leer)".to_string())
}

// ---------- Catálogo de skills (repo Happycapy-skills, MIT) ----------
// Las skills NO se reimplementan: se descargan a <proyecto>/.claude/skills/<nombre>/
// y Claude Code las carga nativamente. Ver docs/skills-integration.md.

const SKILLS_REPO: &str = "happycapy-ai/Happycapy-skills";
const SKILLS_BRANCH: &str = "main";

#[derive(Serialize, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub files: Vec<String>, // rutas relativas dentro de skills/<name>/
}

fn skills_cache_dir() -> PathBuf {
    // %LOCALAPPDATA%\NexoraStudio\skills-cache\skills  (cae a temp si no existe)
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("NexoraStudio").join("skills-cache").join("skills")
}

/// Garantiza el catálogo en caché. Si no existe, descarga el tarball del repo
/// desde codeload.github.com (NO usa la API → no consume el rate-limit de 60/h)
/// y extrae solo la carpeta `skills/`.
fn ensure_skills_cache() -> Result<PathBuf, String> {
    let skills_dir = skills_cache_dir();
    let has_content = std::fs::read_dir(&skills_dir).map(|mut d| d.next().is_some()).unwrap_or(false);
    if has_content {
        return Ok(skills_dir);
    }
    std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;

    let url = format!("https://codeload.github.com/{SKILLS_REPO}/tar.gz/refs/heads/{SKILLS_BRANCH}");
    let resp = ureq::get(&url)
        .set("User-Agent", "nexora-studio")
        .call()
        .map_err(|e| format!("no se pudo descargar el repo de skills: {e}"))?;

    let gz = flate2::read::GzDecoder::new(resp.into_reader());
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries().map_err(|e| e.to_string())? {
        let mut e = entry.map_err(|e| e.to_string())?;
        let path = e.path().map_err(|e| e.to_string())?.into_owned();
        // path = <repo>-<branch>/skills/...  → quitamos el primer componente
        let mut comps = path.components();
        comps.next();
        let rest = comps.as_path();
        let rel = match rest.strip_prefix("skills") {
            Ok(r) => r,
            Err(_) => continue, // fuera de skills/
        };
        let dest = skills_dir.join(rel);
        if e.header().entry_type().is_dir() {
            let _ = std::fs::create_dir_all(&dest);
        } else {
            if let Some(p) = dest.parent() { let _ = std::fs::create_dir_all(p); }
            e.unpack(&dest).map_err(|err| err.to_string())?;
        }
    }
    Ok(skills_dir)
}

fn list_files_rel(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { list_files_rel(&p, base, out); }
            else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u32> {
    std::fs::create_dir_all(dst)?;
    let mut n = 0;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let from = e.path();
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() { n += copy_dir(&from, &to)?; }
        else { std::fs::copy(&from, &to)?; n += 1; }
    }
    Ok(n)
}

/// Catálogo de skills (desde la caché local del tarball).
#[tauri::command]
pub fn skills_catalog() -> Result<Vec<SkillEntry>, String> {
    let skills_dir = ensure_skills_cache()?;
    let mut out: Vec<SkillEntry> = Vec::new();
    for e in std::fs::read_dir(&skills_dir).map_err(|e| e.to_string())? {
        let e = match e { Ok(e) => e, Err(_) => continue };
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
        let name = e.file_name().to_string_lossy().into_owned();
        let mut files = Vec::new();
        list_files_rel(&e.path(), &e.path(), &mut files);
        out.push(SkillEntry { name, files });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Copia una skill de la caché a <proyecto>/.claude/skills/<name>/.
/// `_files` se ignora: copiamos la carpeta completa desde la caché.
#[tauri::command]
pub fn install_skill(project_path: String, name: String, _files: Vec<String>) -> Result<u32, String> {
    let skills_dir = ensure_skills_cache()?;
    let src = skills_dir.join(&name);
    if !src.is_dir() {
        return Err("skill no encontrada en el catálogo".into());
    }
    let dst = PathBuf::from(&project_path).join(".claude").join("skills").join(&name);
    copy_dir(&src, &dst).map_err(|e| e.to_string())
}
