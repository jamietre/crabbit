use anyhow::Context;
use rusqlite::Connection;

pub mod agent;
pub mod auth;
pub mod claude_auth_check;
pub mod prompts;
pub mod repos;
pub mod settings;
pub mod sync;
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
    let _ = conn.execute_batch("ALTER TABLE tasks ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0");
    let _ = conn.execute_batch("ALTER TABLE agent_state ADD COLUMN usage_pct_7d REAL");
    let _ = conn.execute_batch("ALTER TABLE agent_state ADD COLUMN usage_pct_5h REAL");
    let _ = conn.execute_batch("ALTER TABLE agent_state ADD COLUMN usage_reset_at INTEGER");
    // Issue sync migrations
    let _ = conn.execute_batch("ALTER TABLE repos ADD COLUMN labels_require    TEXT");
    let _ = conn.execute_batch("ALTER TABLE repos ADD COLUMN labels_ignore     TEXT");
    let _ = conn.execute_batch("ALTER TABLE repos ADD COLUMN labels_prioritize TEXT");
    let _ = conn.execute_batch("ALTER TABLE repos ADD COLUMN completion_prompt TEXT");
    let _ = conn.execute_batch("ALTER TABLE tasks ADD COLUMN task_type      TEXT NOT NULL DEFAULT 'github_issue'");
    let _ = conn.execute_batch("ALTER TABLE tasks ADD COLUMN issue_labels   TEXT");
    let _ = conn.execute_batch("ALTER TABLE tasks ADD COLUMN is_prioritized INTEGER NOT NULL DEFAULT 0");
    // Migrate label_filter -> labels_require for existing repos
    let _ = conn.execute_batch(
        "UPDATE repos SET labels_require = json_array(label_filter) \
         WHERE label_filter IS NOT NULL AND labels_require IS NULL"
    );
    seed_default_prompts(conn);
    Ok(())
}

fn seed_default_prompts(conn: &Connection) {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let seeds = [
        (
            "triage",
            "",
            "Issue triage",
            "Review the issue carefully before beginning work. Assess the scope and complexity. \
Identify which files and systems are likely affected. Check for any existing related code, \
tests, or prior art in the repository. Flag any ambiguities or missing requirements early.",
        ),
        (
            "plan",
            "",
            "Implementation planning",
            "Before writing code, outline your approach: identify the files to change, the \
data structures or interfaces to add or modify, and any edge cases to handle. Prefer small, \
focused changes over large refactors. Consider backward compatibility and test coverage.",
        ),
        (
            "code",
            "",
            "Code implementation",
            "Write clean, idiomatic code that follows the conventions already present in the \
repository. Add or update tests for the changed behaviour. Run the test suite before creating \
a PR. Keep commits focused and write a clear PR description explaining the what and why.",
        ),
        (
            "code",
            "rust",
            "Rust code guidance",
            "Follow Rust idioms: prefer `?` for error propagation, avoid unnecessary clones, \
use `anyhow` for internal errors and typed errors at API boundaries. Run `cargo clippy` and \
`cargo fmt` before committing. Ensure all public items have doc comments where appropriate.",
        ),
    ];
    for (category, label, name, content) in seeds {
        let _ = conn.execute(
            "INSERT INTO prompts (category, label, name, content, enabled, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
            rusqlite::params![category, label, name, content, now],
        );
    }
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
        assert!(tables.contains(&"prompts".to_string()));
    }

    #[test]
    fn singleton_rows_exist_after_open() {
        let conn = open_db(":memory:").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_state WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn seed_prompts_inserted_on_open() {
        let conn = open_db(":memory:").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0))
            .unwrap();
        assert!(count > 0, "expected seed prompts to be inserted");
    }
}
