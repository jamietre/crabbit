use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};

/// Upsert a GitHub issue as a task.
///
/// - If not in DB: INSERT with status = `queued`.
/// - If in DB with status = `queued`: UPDATE title, body, labels, is_prioritized.
/// - Otherwise: leave unchanged.
///
/// Returns `(created, updated)`.
#[allow(clippy::too_many_arguments)]
pub fn upsert_issue_as_task(
    conn: &Connection,
    repo_id: i64,
    issue_number: i64,
    issue_title: &str,
    issue_url: &str,
    issue_body: &str,
    issue_labels_json: &str,
    is_prioritized: bool,
    now: i64,
) -> anyhow::Result<(bool, bool)> {
    let existing: Option<(i64, String)> = conn.query_row(
        "SELECT id, status FROM tasks WHERE repo_id = ?1 AND issue_number = ?2",
        params![repo_id, issue_number],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .context("upsert_issue: check existing")?;

    match existing {
        None => {
            conn.execute(
                "INSERT INTO tasks (repo_id, issue_number, issue_title, issue_url, issue_body,
                                    status, task_type, issue_labels, is_prioritized, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 'github_issue', ?6, ?7, ?8, ?8)",
                params![repo_id, issue_number, issue_title, issue_url, issue_body,
                        issue_labels_json, is_prioritized as i64, now],
            )
            .context("upsert_issue: insert")?;
            Ok((true, false))
        }
        Some((task_id, ref status)) if status == "queued" => {
            conn.execute(
                "UPDATE tasks SET issue_title = ?1, issue_url = ?2, issue_body = ?3,
                                  issue_labels = ?4, is_prioritized = ?5, updated_at = ?6
                 WHERE id = ?7",
                params![issue_title, issue_url, issue_body,
                        issue_labels_json, is_prioritized as i64, now, task_id],
            )
            .context("upsert_issue: update")?;
            Ok((false, true))
        }
        _ => Ok((false, false)),
    }
}

/// Mark queued tasks as skipped when their GitHub issue numbers are no longer open.
/// Returns number of tasks closed.
pub fn close_stale_queued_tasks(
    conn: &Connection,
    repo_id: i64,
    open_issue_numbers: &[i64],
    now: i64,
) -> anyhow::Result<u32> {
    if open_issue_numbers.is_empty() {
        // No open issues at all — close all queued tasks for this repo
        let n = conn.execute(
            "UPDATE tasks SET status = 'skipped', updated_at = ?1, completed_at = ?1
             WHERE repo_id = ?2 AND status = 'queued'",
            params![now, repo_id],
        )
        .context("close_stale: no open issues")?;
        return Ok(n as u32);
    }

    // Build a temporary table approach using a big IN clause.
    // SQLite supports up to ~999 parameters; split if needed.
    let placeholders: String = open_issue_numbers
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 3))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "UPDATE tasks SET status = 'skipped', updated_at = ?1, completed_at = ?1
         WHERE repo_id = ?2 AND status = 'queued' AND issue_number NOT IN ({placeholders})"
    );

    let mut stmt = conn.prepare(&sql).context("close_stale: prepare")?;
    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(now),
        Box::new(repo_id),
    ];
    for n in open_issue_numbers {
        bound.push(Box::new(*n));
    }
    let params: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let changed = stmt.execute(params.as_slice()).context("close_stale: execute")?;
    Ok(changed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_db, repos::insert_repo};

    fn now() -> i64 { 1_700_000_000 }

    fn setup() -> (rusqlite::Connection, i64) {
        let conn = open_db(":memory:").unwrap();
        let repo_id = insert_repo(&conn, "acme", "api", None, &[], &[], &[], None, now()).unwrap();
        (conn, repo_id)
    }

    #[test]
    fn upsert_creates_new_task() {
        let (conn, repo_id) = setup();
        let (created, updated) = upsert_issue_as_task(
            &conn, repo_id, 1, "Fix bug", "https://gh/1", "body", "[]", false, now()
        ).unwrap();
        assert!(created);
        assert!(!updated);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE repo_id = ?1", params![repo_id], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn upsert_updates_queued_task() {
        let (conn, repo_id) = setup();
        upsert_issue_as_task(&conn, repo_id, 1, "Fix bug", "https://gh/1", "old body", "[]", false, now()).unwrap();
        let (created, updated) = upsert_issue_as_task(
            &conn, repo_id, 1, "Fix bug v2", "https://gh/1", "new body", "[\"crabbit\"]", true, now() + 1
        ).unwrap();
        assert!(!created);
        assert!(updated);
    }

    #[test]
    fn upsert_skips_non_queued_task() {
        let (conn, repo_id) = setup();
        upsert_issue_as_task(&conn, repo_id, 1, "Fix bug", "https://gh/1", "body", "[]", false, now()).unwrap();
        conn.execute("UPDATE tasks SET status = 'in_progress' WHERE repo_id = ?1 AND issue_number = 1", params![repo_id]).unwrap();
        let (created, updated) = upsert_issue_as_task(
            &conn, repo_id, 1, "Fix bug updated", "https://gh/1", "new body", "[]", false, now() + 1
        ).unwrap();
        assert!(!created);
        assert!(!updated);
    }

    #[test]
    fn close_stale_marks_skipped() {
        let (conn, repo_id) = setup();
        upsert_issue_as_task(&conn, repo_id, 1, "t1", "u1", "b", "[]", false, now()).unwrap();
        upsert_issue_as_task(&conn, repo_id, 2, "t2", "u2", "b", "[]", false, now()).unwrap();
        // issue 2 is still open, issue 1 is now closed
        let closed = close_stale_queued_tasks(&conn, repo_id, &[2], now() + 1).unwrap();
        assert_eq!(closed, 1);
        let status: String = conn.query_row(
            "SELECT status FROM tasks WHERE issue_number = 1", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(status, "skipped");
    }
}
