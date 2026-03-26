use anyhow::Context;
use rusqlite::Connection;

pub mod agent;
pub mod auth;
pub mod claude_auth;
pub mod repos;
pub mod settings;
pub mod tasks;

const SCHEMA: &str = include_str!("schema.sql");

pub fn open_db(path: &str) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("cannot open database at {}", path))?;
    // Enable WAL mode and foreign keys
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .context("failed to set pragmas")?;
    run_schema(&conn)?;
    Ok(conn)
}

fn run_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA).context("failed to run schema")?;
    // Migrations: ignore errors when columns already exist
    let _ = conn.execute_batch("ALTER TABLE claude_settings ADD COLUMN usage_limit_pct REAL");
    let _ = conn.execute_batch("ALTER TABLE agent_state ADD COLUMN usage_pct_7d REAL");
    let _ = conn.execute_batch("ALTER TABLE agent_state ADD COLUMN usage_reset_at INTEGER");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_all_tables() {
        let conn = open_db(":memory:").unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(tables.contains(&"repos".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"task_events".to_string()));
        assert!(tables.contains(&"agent_state".to_string()));
        assert!(tables.contains(&"github_auth".to_string()));
        assert!(tables.contains(&"claude_settings".to_string()));
    }

    #[test]
    fn singleton_rows_exist_after_open() {
        let conn = open_db(":memory:").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_state WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
