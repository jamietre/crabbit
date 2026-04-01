use anyhow::Context;
use crabbit_common::Repo;
use rusqlite::{params, Connection, OptionalExtension};

const SELECT_COLS: &str =
    "id, owner, name, enabled, label_filter, labels_require, labels_ignore, labels_prioritize, completion_prompt, created_at";

#[allow(clippy::too_many_arguments)]
pub fn insert_repo(
    conn: &Connection,
    owner: &str,
    name: &str,
    label_filter: Option<&str>,
    labels_require: &[String],
    labels_ignore: &[String],
    labels_prioritize: &[String],
    completion_prompt: Option<&str>,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO repos (owner, name, label_filter, labels_require, labels_ignore, labels_prioritize, completion_prompt, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            owner, name, label_filter,
            to_json_array(labels_require),
            to_json_array(labels_ignore),
            to_json_array(labels_prioritize),
            completion_prompt,
            now
        ],
    )
    .context("insert_repo")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(&format!("SELECT {SELECT_COLS} FROM repos ORDER BY id"))?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn list_enabled_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(&format!("SELECT {SELECT_COLS} FROM repos WHERE enabled = 1 ORDER BY id"))?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn get_repo(conn: &Connection, id: i64) -> anyhow::Result<Option<Repo>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM repos WHERE id = ?1"),
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

pub fn update_repo_labels(
    conn: &Connection,
    id: i64,
    labels_require: Option<&[String]>,
    labels_ignore: Option<&[String]>,
    labels_prioritize: Option<&[String]>,
    completion_prompt: Option<Option<&str>>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE repos SET
            labels_require    = CASE WHEN ?1 THEN ?2 ELSE labels_require END,
            labels_ignore     = CASE WHEN ?3 THEN ?4 ELSE labels_ignore END,
            labels_prioritize = CASE WHEN ?5 THEN ?6 ELSE labels_prioritize END,
            completion_prompt = CASE WHEN ?7 THEN ?8 ELSE completion_prompt END
         WHERE id = ?9",
        params![
            labels_require.is_some(), labels_require.map(to_json_array),
            labels_ignore.is_some(), labels_ignore.map(to_json_array),
            labels_prioritize.is_some(), labels_prioritize.map(to_json_array),
            completion_prompt.is_some(), completion_prompt.flatten(),
            id
        ],
    )
    .context("update_repo_labels")?;
    Ok(())
}

pub fn delete_repo(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
        .context("delete_repo")?;
    Ok(())
}

fn to_json_array(v: &[String]) -> Option<String> {
    if v.is_empty() { None } else { serde_json::to_string(v).ok() }
}

fn parse_json_array(s: Option<String>) -> Vec<String> {
    s.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn row_to_repo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        owner: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        label_filter: row.get(4)?,
        labels_require: parse_json_array(row.get(5)?),
        labels_ignore: parse_json_array(row.get(6)?),
        labels_prioritize: parse_json_array(row.get(7)?),
        completion_prompt: row.get(8)?,
        created_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    fn now() -> i64 { 1_700_000_000 }

    fn insert_simple(conn: &Connection, owner: &str, name: &str) -> i64 {
        insert_repo(conn, owner, name, None, &[], &[], &[], None, now()).unwrap()
    }

    #[test]
    fn insert_and_list() {
        let conn = open_db(":memory:").unwrap();
        insert_simple(&conn, "acme", "api");
        insert_repo(&conn, "acme", "web", Some("crabbit"),
                    &["crabbit".into()], &[], &[], None, now()).unwrap();
        let repos = list_repos(&conn).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].owner, "acme");
        assert_eq!(repos[1].label_filter, Some("crabbit".into()));
        assert_eq!(repos[1].labels_require, vec!["crabbit".to_string()]);
    }

    #[test]
    fn insert_duplicate_fails() {
        let conn = open_db(":memory:").unwrap();
        insert_simple(&conn, "acme", "api");
        assert!(insert_repo(&conn, "acme", "api", None, &[], &[], &[], None, now()).is_err());
    }

    #[test]
    fn update_repo() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_simple(&conn, "acme", "api");
        set_repo_enabled(&conn, id, false).unwrap();
        let repo = get_repo(&conn, id).unwrap().unwrap();
        assert!(!repo.enabled);
    }

    #[test]
    fn update_labels() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_simple(&conn, "acme", "api");
        update_repo_labels(&conn, id,
            Some(&["crabbit".into()]),
            Some(&["human".into()]),
            Some(&["urgent".into()]),
            Some(Some("Create a PR.")),
        ).unwrap();
        let repo = get_repo(&conn, id).unwrap().unwrap();
        assert_eq!(repo.labels_require, vec!["crabbit".to_string()]);
        assert_eq!(repo.labels_ignore, vec!["human".to_string()]);
        assert_eq!(repo.labels_prioritize, vec!["urgent".to_string()]);
        assert_eq!(repo.completion_prompt, Some("Create a PR.".into()));
    }

    #[test]
    fn delete_repo_test() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_simple(&conn, "acme", "api");
        delete_repo(&conn, id).unwrap();
        assert!(get_repo(&conn, id).unwrap().is_none());
    }
}
