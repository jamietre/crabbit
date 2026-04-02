use anyhow::Context;
use crabbit_common::{AgentState, AgentStatus, UpdateAgentStateRequest};
use rusqlite::{params, Connection};

pub fn get_agent_state(conn: &Connection) -> anyhow::Result<AgentState> {
    conn.query_row(
        "SELECT status, wake_at, last_run_at, current_task_id, usage_note, usage_pct_7d, usage_pct_5h, usage_reset_at
         FROM agent_state WHERE id = 1",
        [],
        |row| {
            let status_str: String = row.get(0)?;
            let status = match status_str.as_str() {
                "running" => AgentStatus::Running,
                "sleeping" => AgentStatus::Sleeping,
                _ => AgentStatus::Idle,
            };
            Ok(AgentState {
                status,
                wake_at: row.get(1)?,
                last_run_at: row.get(2)?,
                current_task_id: row.get(3)?,
                usage_note: row.get(4)?,
                usage_pct_7d: row.get(5)?,
                usage_pct_5h: row.get(6)?,
                usage_reset_at: row.get(7)?,
            })
        },
    )
    .context("get_agent_state")
}

pub fn set_agent_state(conn: &Connection, req: &UpdateAgentStateRequest) -> anyhow::Result<()> {
    let status_str = req.status.as_ref().map(|s| match s {
        AgentStatus::Idle => "idle",
        AgentStatus::Running => "running",
        AgentStatus::Sleeping => "sleeping",
    });
    conn.execute(
        "UPDATE agent_state SET
            status          = COALESCE(?1, status),
            wake_at         = CASE WHEN ?1 IS NOT NULL THEN ?2 ELSE wake_at END,
            current_task_id = CASE WHEN ?3 IS NOT NULL THEN ?3 ELSE current_task_id END,
            usage_note      = CASE WHEN ?4 IS NOT NULL THEN ?4 ELSE usage_note END,
            usage_pct_7d    = CASE WHEN ?5 IS NOT NULL THEN ?5 ELSE usage_pct_7d END,
            usage_pct_5h    = CASE WHEN ?6 IS NOT NULL THEN ?6 ELSE usage_pct_5h END,
            usage_reset_at  = CASE WHEN ?7 IS NOT NULL THEN ?7 ELSE usage_reset_at END,
            last_run_at     = CASE WHEN ?1 = 'running' THEN strftime('%s','now') ELSE last_run_at END
         WHERE id = 1",
        params![status_str, req.wake_at, req.current_task_id, req.usage_note, req.usage_pct_7d, req.usage_pct_5h, req.usage_reset_at],
    )
    .context("set_agent_state")?;
    Ok(())
}

pub fn recover_agent_state(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE agent_state SET status = 'idle', current_task_id = NULL
         WHERE status = 'running'",
        [],
    ).context("recover_agent_state")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crabbit_common::{AgentStatus, UpdateAgentStateRequest};

    #[test]
    fn default_agent_state_is_idle() {
        let conn = open_db(":memory:").unwrap();
        let state = get_agent_state(&conn).unwrap();
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.wake_at.is_none());
    }

    #[test]
    fn recover_resets_running_to_idle() {
        let conn = open_db(":memory:").unwrap();
        set_agent_state(&conn, &UpdateAgentStateRequest {
            status: Some(AgentStatus::Running),
            wake_at: None,
            current_task_id: None,
            usage_note: None,
            usage_pct_7d: None,
            usage_pct_5h: None,
            usage_reset_at: None,
        }).unwrap();
        recover_agent_state(&conn).unwrap();
        let state = get_agent_state(&conn).unwrap();
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.current_task_id.is_none());
    }

    #[test]
    fn set_agent_sleeping() {
        let conn = open_db(":memory:").unwrap();
        set_agent_state(&conn, &UpdateAgentStateRequest {
            status: Some(AgentStatus::Sleeping),
            wake_at: Some(9_999_999_999),
            current_task_id: None,
            usage_note: Some("hit limit".into()),
            usage_pct_7d: None,
            usage_pct_5h: None,
            usage_reset_at: None,
        }).unwrap();
        let state = get_agent_state(&conn).unwrap();
        assert_eq!(state.status, AgentStatus::Sleeping);
        assert_eq!(state.wake_at, Some(9_999_999_999));
    }
}
