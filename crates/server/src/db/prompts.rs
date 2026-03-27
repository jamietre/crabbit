use anyhow::Context;
use crabbit_common::Prompt;
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_prompt(
    conn: &Connection,
    category: &str,
    label: &str,
    name: &str,
    content: &str,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO prompts (category, label, name, content, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![category, label, name, content, now],
    )
    .context("insert_prompt")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_prompts(conn: &Connection) -> anyhow::Result<Vec<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, category, label, name, content, enabled, created_at, updated_at
         FROM prompts ORDER BY category, label, name",
    )?;
    let rows = stmt.query_map([], row_to_prompt)?;
    rows.map(|r| r.context("row_to_prompt")).collect()
}

pub fn list_prompts_by_category(conn: &Connection, category: &str) -> anyhow::Result<Vec<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, category, label, name, content, enabled, created_at, updated_at
         FROM prompts WHERE category = ?1 AND enabled = 1 ORDER BY label, name",
    )?;
    let rows = stmt.query_map(params![category], row_to_prompt)?;
    rows.map(|r| r.context("row_to_prompt")).collect()
}

pub fn get_prompt(conn: &Connection, id: i64) -> anyhow::Result<Option<Prompt>> {
    conn.query_row(
        "SELECT id, category, label, name, content, enabled, created_at, updated_at
         FROM prompts WHERE id = ?1",
        params![id],
        row_to_prompt,
    )
    .optional()
    .context("get_prompt")
}

pub fn update_prompt(
    conn: &Connection,
    id: i64,
    category: Option<&str>,
    label: Option<&str>,
    name: Option<&str>,
    content: Option<&str>,
    enabled: Option<bool>,
    now: i64,
) -> anyhow::Result<()> {
    if let Some(v) = category {
        conn.execute("UPDATE prompts SET category = ?1, updated_at = ?2 WHERE id = ?3", params![v, now, id])
            .context("update category")?;
    }
    if let Some(v) = label {
        conn.execute("UPDATE prompts SET label = ?1, updated_at = ?2 WHERE id = ?3", params![v, now, id])
            .context("update label")?;
    }
    if let Some(v) = name {
        conn.execute("UPDATE prompts SET name = ?1, updated_at = ?2 WHERE id = ?3", params![v, now, id])
            .context("update name")?;
    }
    if let Some(v) = content {
        conn.execute("UPDATE prompts SET content = ?1, updated_at = ?2 WHERE id = ?3", params![v, now, id])
            .context("update content")?;
    }
    if let Some(v) = enabled {
        conn.execute("UPDATE prompts SET enabled = ?1, updated_at = ?2 WHERE id = ?3", params![v as i64, now, id])
            .context("update enabled")?;
    }
    Ok(())
}

pub fn delete_prompt(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM prompts WHERE id = ?1", params![id])
        .context("delete_prompt")?;
    Ok(())
}

fn row_to_prompt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        category: row.get(1)?,
        label: row.get(2)?,
        name: row.get(3)?,
        content: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
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
        let before = list_prompts(&conn).unwrap().len();
        insert_prompt(&conn, "triage", "", "Extra triage", "content here", now()).unwrap();
        insert_prompt(&conn, "code", "rust", "Extra rust", "rust content", now()).unwrap();
        let prompts = list_prompts(&conn).unwrap();
        assert_eq!(prompts.len(), before + 2);
    }

    #[test]
    fn list_by_category() {
        let conn = open_db(":memory:").unwrap();
        let before = list_prompts_by_category(&conn, "code").unwrap().len();
        insert_prompt(&conn, "code", "go", "Go prompt", "go content", now()).unwrap();
        let code_prompts = list_prompts_by_category(&conn, "code").unwrap();
        assert_eq!(code_prompts.len(), before + 1);
        assert!(code_prompts.iter().all(|p| p.category == "code"));
    }

    #[test]
    fn update_prompt_fields() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_prompt(&conn, "triage", "", "Old name", "old content", now()).unwrap();
        update_prompt(&conn, id, None, None, Some("New name"), Some("new content"), None, now() + 1).unwrap();
        let p = get_prompt(&conn, id).unwrap().unwrap();
        assert_eq!(p.name, "New name");
        assert_eq!(p.content, "new content");
        assert!(p.enabled);
    }

    #[test]
    fn delete_prompt_test() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_prompt(&conn, "plan", "", "Plan prompt", "plan content", now()).unwrap();
        delete_prompt(&conn, id).unwrap();
        assert!(get_prompt(&conn, id).unwrap().is_none());
    }
}
