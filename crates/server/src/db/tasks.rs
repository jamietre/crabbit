use anyhow::Context;
use crabbit_common::{Task, TaskEvent, TaskStatus};
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_task(
    conn: &Connection,
    repo_id: i64,
    issue_number: i64,
    issue_title: &str,
    issue_url: &str,
    issue_body: &str,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO tasks (repo_id, issue_number, issue_title, issue_url, issue_body,
                            status, task_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 'github_issue', ?6, ?6)",
        params![repo_id, issue_number, issue_title, issue_url, issue_body, now],
    )
    .context("insert_task")?;
    Ok(conn.last_insert_rowid())
}

pub fn get_task(conn: &Connection, id: i64) -> anyhow::Result<Option<Task>> {
    conn.query_row(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, task_type, issue_labels, is_prioritized,
                pr_url, pr_number, error_message, claude_session_id, retry_count,
                created_at, updated_at, started_at, completed_at
         FROM tasks WHERE id = ?1",
        params![id],
        row_to_task,
    )
    .optional()
    .context("get_task")
}

pub fn get_task_by_issue(conn: &Connection, repo_id: i64, issue_number: i64) -> anyhow::Result<Option<Task>> {
    conn.query_row(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, task_type, issue_labels, is_prioritized,
                pr_url, pr_number, error_message, claude_session_id, retry_count,
                created_at, updated_at, started_at, completed_at
         FROM tasks WHERE repo_id = ?1 AND issue_number = ?2",
        params![repo_id, issue_number],
        row_to_task,
    )
    .optional()
    .context("get_task_by_issue")
}

pub fn list_tasks(
    conn: &Connection,
    status: Option<&TaskStatus>,
    repo_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<Task>> {
    let status_str = status.map(|s| s.to_string());
    let mut stmt = conn.prepare(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, task_type, issue_labels, is_prioritized,
                pr_url, pr_number, error_message, claude_session_id, retry_count,
                created_at, updated_at, started_at, completed_at
         FROM tasks
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR repo_id = ?2)
         ORDER BY created_at DESC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt.query_map(params![status_str, repo_id, limit, offset], row_to_task)?;
    rows.map(|r| r.context("row_to_task")).collect()
}

pub fn increment_retry_count(conn: &Connection, id: i64, now: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE tasks SET retry_count = retry_count + 1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .context("increment_retry_count")?;
    Ok(())
}

pub fn delete_task(conn: &Connection, id: i64) -> anyhow::Result<bool> {
    // Clear agent_state reference to avoid FK violation
    conn.execute(
        "UPDATE agent_state SET current_task_id = NULL WHERE current_task_id = ?1",
        params![id],
    )
    .context("delete_task: clear agent_state")?;
    let n = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .context("delete_task")?;
    Ok(n > 0)
}

pub fn update_task_status(
    conn: &Connection,
    id: i64,
    status: &TaskStatus,
    now: i64,
) -> anyhow::Result<()> {
    let started_at_set = matches!(status, TaskStatus::InProgress);
    let completed_at_set = matches!(
        status,
        TaskStatus::PrCreated | TaskStatus::NeedsHuman | TaskStatus::Failed | TaskStatus::Skipped
    );
    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2,
                started_at = CASE WHEN ?3 THEN ?2 ELSE started_at END,
                completed_at = CASE WHEN ?4 THEN ?2 ELSE completed_at END
         WHERE id = ?5",
        params![status.to_string(), now, started_at_set, completed_at_set, id],
    )
    .context("update_task_status")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update_task_outcome(
    conn: &Connection,
    id: i64,
    status: &TaskStatus,
    pr_url: Option<&str>,
    pr_number: Option<i64>,
    error_message: Option<&str>,
    claude_session_id: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?1, pr_url = ?2, pr_number = ?3,
                error_message = ?4, claude_session_id = ?5, updated_at = ?6,
                completed_at = ?6
         WHERE id = ?7",
        params![status.to_string(), pr_url, pr_number, error_message, claude_session_id, now, id],
    )
    .context("update_task_outcome")?;
    Ok(())
}

pub fn insert_task_event(
    conn: &Connection,
    task_id: i64,
    event_type: &str,
    payload: &serde_json::Value,
    now: i64,
) -> anyhow::Result<i64> {
    let payload_str = serde_json::to_string(payload)?;
    conn.execute(
        "INSERT INTO task_events (task_id, event_type, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, event_type, payload_str, now],
    )
    .context("insert_task_event")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_task_events(conn: &Connection, task_id: i64) -> anyhow::Result<Vec<TaskEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, event_type, payload, created_at
         FROM task_events WHERE task_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows: Vec<_> = stmt.query_map(params![task_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?
    .collect::<rusqlite::Result<_>>()?;

    rows.into_iter()
        .map(|(id, task_id, event_type, payload_str, created_at)| {
            let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
            Ok(TaskEvent { id, task_id, event_type, payload, created_at })
        })
        .collect()
}

/// Pick the highest-priority actionable task from the local queue.
/// Priority order: retrying > pending > is_prioritized queued > queued (oldest first within tier).
pub fn next_queued_task(conn: &Connection) -> anyhow::Result<Option<Task>> {
    conn.query_row(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, task_type, issue_labels, is_prioritized,
                pr_url, pr_number, error_message, claude_session_id, retry_count,
                created_at, updated_at, started_at, completed_at
         FROM tasks
         WHERE status IN ('retrying', 'pending', 'queued')
         ORDER BY
           CASE status WHEN 'retrying' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END ASC,
           is_prioritized DESC,
           created_at ASC
         LIMIT 1",
        [],
        row_to_task,
    )
    .optional()
    .context("next_queued_task")
}

pub fn reset_in_progress_tasks(conn: &Connection) -> anyhow::Result<usize> {
    let n = conn.execute(
        "UPDATE tasks SET status = 'pending', updated_at = strftime('%s','now')
         WHERE status = 'in_progress'",
        [],
    ).context("reset_in_progress_tasks")?;
    Ok(n)
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status_str: String = row.get(6)?;
    let status: TaskStatus = status_str.parse().unwrap_or(TaskStatus::Queued);
    let labels_json: Option<String> = row.get(8)?;
    let issue_labels: Vec<String> = labels_json
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    Ok(Task {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        issue_number: row.get(2)?,
        issue_title: row.get(3)?,
        issue_url: row.get(4)?,
        issue_body: row.get(5)?,
        status,
        task_type: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "github_issue".into()),
        issue_labels,
        is_prioritized: row.get::<_, i64>(9)? != 0,
        pr_url: row.get(10)?,
        pr_number: row.get(11)?,
        error_message: row.get(12)?,
        claude_session_id: row.get(13)?,
        retry_count: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        started_at: row.get(17)?,
        completed_at: row.get(18)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_db, repos::insert_repo};
    use crabbit_common::TaskStatus;

    fn setup() -> (rusqlite::Connection, i64) {
        let conn = open_db(":memory:").unwrap();
        let repo_id = insert_repo(&conn, "acme", "api", None, &[], &[], &[], None, 1_700_000_000).unwrap();
        (conn, repo_id)
    }

    #[test]
    fn insert_and_get_task() {
        let (conn, repo_id) = setup();
        let id = insert_task(&conn, repo_id, 42, "Fix bug", "https://gh/42", "body", 1_700_000_000).unwrap();
        let t = get_task(&conn, id).unwrap().unwrap();
        assert_eq!(t.issue_number, 42);
        assert_eq!(t.status, TaskStatus::Pending); // insert_task uses 'pending' explicitly
    }

    #[test]
    fn update_task_status_test() {
        let (conn, repo_id) = setup();
        let id = insert_task(&conn, repo_id, 1, "t", "u", "b", 1_700_000_000).unwrap();
        update_task_status(&conn, id, &TaskStatus::InProgress, 1_700_000_001).unwrap();
        let t = get_task(&conn, id).unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::InProgress);
    }

    #[test]
    fn list_tasks_by_status() {
        let (conn, repo_id) = setup();
        insert_task(&conn, repo_id, 1, "t1", "u1", "b", 1_700_000_000).unwrap();
        let id2 = insert_task(&conn, repo_id, 2, "t2", "u2", "b", 1_700_000_000).unwrap();
        update_task_status(&conn, id2, &TaskStatus::PrCreated, 1_700_000_001).unwrap();
        let pending = list_tasks(&conn, Some(&TaskStatus::Pending), None, 100, 0).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].issue_number, 1);
    }

    #[test]
    fn reset_in_progress_resets_to_pending() {
        let (conn, repo_id) = setup();
        let id = insert_task(&conn, repo_id, 1, "t", "u", "b", 1_700_000_000).unwrap();
        update_task_status(&conn, id, &TaskStatus::InProgress, 1_700_000_001).unwrap();
        let n = reset_in_progress_tasks(&conn).unwrap();
        assert_eq!(n, 1);
        let t = get_task(&conn, id).unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Pending);
    }

    #[test]
    fn insert_and_list_events() {
        let (conn, repo_id) = setup();
        let task_id = insert_task(&conn, repo_id, 1, "t", "u", "b", 1_700_000_000).unwrap();
        insert_task_event(&conn, task_id, "claude_output", &serde_json::json!({"text": "hello"}), 1_700_000_001).unwrap();
        let events = list_task_events(&conn, task_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "claude_output");
    }
}
