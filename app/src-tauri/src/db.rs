//! Persistencia local (SQLite vía rusqlite). Solo datos operativos —
//! NUNCA credenciales ni tokens (principio I). Esquema según data-model.md.

use rusqlite::{params, Connection, Result};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Abre (o crea) la base de datos e inicializa el esquema.
pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS project (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            base_branch TEXT NOT NULL DEFAULT 'main',
            opened_at INTEGER,
            last_active_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS agent (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            capabilities TEXT NOT NULL DEFAULT '[]'
         );
         CREATE TABLE IF NOT EXISTS task (
            id TEXT PRIMARY KEY,
            project_id TEXT,
            agent_id TEXT,
            description TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            worktree_path TEXT,
            branch TEXT,
            depends_on TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER,
            started_at INTEGER,
            ended_at INTEGER,
            cost_usd REAL,
            error TEXT
         );
         CREATE TABLE IF NOT EXISTS agent_event (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload TEXT,
            ts INTEGER
         );",
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO agent (id, name, capabilities) VALUES (?1, ?2, ?3)",
        params!["codex", "Codex CLI", r#"["backend","architecture","tests","refactor"]"#],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO agent (id, name, capabilities) VALUES (?1, ?2, ?3)",
        params!["claude", "Claude Code", r#"["frontend","ui","debugging","docs","review"]"#],
    )?;
    Ok(())
}

pub fn upsert_project(conn: &Connection, id: &str, path: &str, base_branch: &str) -> Result<()> {
    let t = now_ms();
    conn.execute(
        "INSERT INTO project (id, path, base_branch, opened_at, last_active_at) VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET last_active_at = ?4, base_branch = ?3",
        params![id, path, base_branch, t],
    )?;
    Ok(())
}

pub fn insert_task(
    conn: &Connection,
    id: &str,
    project_id: Option<&str>,
    agent_id: &str,
    description: &str,
    status: &str,
) -> Result<()> {
    let t = now_ms();
    conn.execute(
        "INSERT INTO task (id, project_id, agent_id, description, status, created_at, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, project_id, agent_id, description, status, t],
    )?;
    Ok(())
}

pub fn set_task_status(
    conn: &Connection,
    id: &str,
    status: &str,
    cost: Option<f64>,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE task SET status = ?2, ended_at = ?3,
            cost_usd = COALESCE(?4, cost_usd), error = COALESCE(?5, error) WHERE id = ?1",
        params![id, status, now_ms(), cost, error],
    )?;
    Ok(())
}

pub fn insert_event(conn: &Connection, task_id: &str, kind: &str, payload: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO agent_event (task_id, kind, payload, ts) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, kind, payload, now_ms()],
    )?;
    Ok(())
}

#[derive(Serialize)]
pub struct TaskRow {
    pub id: String,
    pub agent_id: Option<String>,
    pub description: String,
    pub status: String,
    pub cost_usd: Option<f64>,
}

pub fn recent_tasks(conn: &Connection, limit: i64) -> Result<Vec<TaskRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, description, status, cost_usd FROM task ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(TaskRow {
            id: r.get(0)?,
            agent_id: r.get(1)?,
            description: r.get(2)?,
            status: r.get(3)?,
            cost_usd: r.get(4)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_seeds_agents_and_accepts_task() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let agents: i64 = conn.query_row("SELECT COUNT(*) FROM agent", [], |r| r.get(0)).unwrap();
        assert_eq!(agents, 2);
        init_schema(&conn).unwrap(); // idempotente
        let agents2: i64 = conn.query_row("SELECT COUNT(*) FROM agent", [], |r| r.get(0)).unwrap();
        assert_eq!(agents2, 2);
    }

    #[test]
    fn tasks_persist_and_list_recent() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_task(&conn, "t1", None, "codex", "tarea uno", "running").unwrap();
        insert_task(&conn, "t2", None, "claude", "tarea dos", "running").unwrap();
        set_task_status(&conn, "t1", "done", Some(0.05), None).unwrap();
        insert_event(&conn, "t1", "step", r#"{"kind":"step","text":"hola"}"#).unwrap();

        let rows = recent_tasks(&conn, 10).unwrap();
        assert_eq!(rows.len(), 2);
        let t1 = rows.iter().find(|r| r.id == "t1").unwrap();
        assert_eq!(t1.status, "done");
        assert_eq!(t1.cost_usd, Some(0.05));

        let events: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_event WHERE task_id='t1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(events, 1);
    }
}
