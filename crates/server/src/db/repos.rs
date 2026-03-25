use anyhow::Context;
use crabbit_common::Repo;
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_repo(
    conn: &Connection,
    owner: &str,
    name: &str,
    label_filter: Option<&str>,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO repos (owner, name, label_filter, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![owner, name, label_filter, now],
    )
    .context("insert_repo")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn list_enabled_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos WHERE enabled = 1 ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn get_repo(conn: &Connection, id: i64) -> anyhow::Result<Option<Repo>> {
    conn.query_row(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos WHERE id = ?1",
        params![id],
        row_to_repo,
    )
    .optional()
    .context("get_repo")
}

pub fn set_repo_enabled(conn: &Connection, id: i64, enabled: bool) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE repos SET enabled = ?1 WHERE id = ?2",
        params![enabled as i64, id],
    )
    .context("set_repo_enabled")?;
    Ok(())
}

pub fn set_repo_label_filter(
    conn: &Connection,
    id: i64,
    label_filter: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE repos SET label_filter = ?1 WHERE id = ?2",
        params![label_filter, id],
    )
    .context("set_repo_label_filter")?;
    Ok(())
}

pub fn delete_repo(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
        .context("delete_repo")?;
    Ok(())
}

fn row_to_repo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        owner: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        label_filter: row.get(4)?,
        created_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    fn now() -> i64 { 1_700_000_000 }

    #[test]
    fn insert_and_list() {
        let conn = open_db(":memory:").unwrap();
        insert_repo(&conn, "acme", "api", None, now()).unwrap();
        insert_repo(&conn, "acme", "web", Some("crabbit"), now()).unwrap();
        let repos = list_repos(&conn).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].owner, "acme");
        assert_eq!(repos[1].label_filter, Some("crabbit".into()));
    }

    #[test]
    fn insert_duplicate_fails() {
        let conn = open_db(":memory:").unwrap();
        insert_repo(&conn, "acme", "api", None, now()).unwrap();
        assert!(insert_repo(&conn, "acme", "api", None, now()).is_err());
    }

    #[test]
    fn update_repo() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_repo(&conn, "acme", "api", None, now()).unwrap();
        set_repo_enabled(&conn, id, false).unwrap();
        let repo = get_repo(&conn, id).unwrap().unwrap();
        assert!(!repo.enabled);
    }

    #[test]
    fn delete_repo_test() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_repo(&conn, "acme", "api", None, now()).unwrap();
        delete_repo(&conn, id).unwrap();
        assert!(get_repo(&conn, id).unwrap().is_none());
    }
}
