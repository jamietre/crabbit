use anyhow::Context;
use crabbit_common::Toolchain;
use rusqlite::{params, Connection, OptionalExtension};

pub fn list_toolchains(conn: &Connection) -> anyhow::Result<Vec<Toolchain>> {
    let mut stmt = conn.prepare(
        "SELECT name, display_name, image, image_status, install_steps, \
         detection_markers, builtin, build_log, created_at \
         FROM toolchains ORDER BY name",
    )?;
    let rows = stmt.query_map([], row_to_toolchain)?;
    rows.map(|r| r.context("row_to_toolchain")).collect()
}

pub fn get_toolchain(conn: &Connection, name: &str) -> anyhow::Result<Option<Toolchain>> {
    conn.query_row(
        "SELECT name, display_name, image, image_status, install_steps, \
         detection_markers, builtin, build_log, created_at \
         FROM toolchains WHERE name = ?1",
        params![name],
        row_to_toolchain,
    )
    .optional()
    .context("get_toolchain")
}

pub fn insert_toolchain(conn: &Connection, tc: &Toolchain) -> anyhow::Result<()> {
    let steps = serde_json::to_string(&tc.install_steps).unwrap_or_else(|_| "[]".into());
    let markers = serde_json::to_string(&tc.detection_markers).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO toolchains \
         (name, display_name, image, image_status, builtin, install_steps, detection_markers, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            tc.name, tc.display_name, tc.image, tc.image_status,
            tc.builtin as i64, steps, markers, tc.created_at
        ],
    )
    .context("insert_toolchain")?;
    Ok(())
}

pub fn set_image_status(conn: &Connection, name: &str, status: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE toolchains SET image_status = ?1 WHERE name = ?2",
        params![status, name],
    )
    .context("set_image_status")?;
    Ok(())
}

pub fn append_build_log(conn: &Connection, name: &str, line: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE toolchains SET build_log = COALESCE(build_log, '') || ?1 || char(10) WHERE name = ?2",
        params![line, name],
    )
    .context("append_build_log")?;
    Ok(())
}

pub fn clear_build_log(conn: &Connection, name: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE toolchains SET build_log = NULL WHERE name = ?1",
        params![name],
    )
    .context("clear_build_log")?;
    Ok(())
}

pub fn update_install_steps(conn: &Connection, name: &str, steps: &[String]) -> anyhow::Result<()> {
    let json = serde_json::to_string(steps).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE toolchains SET install_steps = ?1 WHERE name = ?2",
        params![json, name],
    )
    .context("update_install_steps")?;
    Ok(())
}

pub fn count_repos_using(conn: &Connection, name: &str) -> anyhow::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE toolchain = ?1",
        params![name],
        |row| row.get(0),
    )
    .context("count_repos_using")
}

pub fn delete_toolchain(conn: &Connection, name: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM toolchains WHERE name = ?1", params![name])
        .context("delete_toolchain")?;
    Ok(())
}

fn row_to_toolchain(row: &rusqlite::Row<'_>) -> rusqlite::Result<Toolchain> {
    let install_steps: String = row.get(4)?;
    let detection_markers: String = row.get(5)?;
    Ok(Toolchain {
        name: row.get(0)?,
        display_name: row.get(1)?,
        image: row.get(2)?,
        image_status: row.get(3)?,
        install_steps: serde_json::from_str(&install_steps).unwrap_or_default(),
        detection_markers: serde_json::from_str(&detection_markers).unwrap_or_default(),
        builtin: row.get::<_, i64>(6)? != 0,
        build_log: row.get(7)?,
        created_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    fn now() -> i64 { 1_700_000_000 }

    fn make_tc(name: &str) -> Toolchain {
        Toolchain {
            name: name.into(),
            display_name: "Test".into(),
            image: format!("crabbit-{name}:local"),
            image_status: "pending".into(),
            builtin: false,
            install_steps: vec!["apt-get install -y foo".into()],
            detection_markers: vec!["foo.lock".into()],
            build_log: None,
            created_at: now(),
        }
    }

    #[test]
    fn seeded_toolchains_returned_by_list() {
        let conn = open_db(":memory:").unwrap();
        let tcs = list_toolchains(&conn).unwrap();
        assert!(tcs.iter().any(|t| t.name == "node"));
        assert!(tcs.iter().any(|t| t.name == "rust"));
        assert!(tcs.iter().all(|t| t.builtin));
    }

    #[test]
    fn insert_and_get() {
        let conn = open_db(":memory:").unwrap();
        insert_toolchain(&conn, &make_tc("elixir")).unwrap();
        let tc = get_toolchain(&conn, "elixir").unwrap().unwrap();
        assert_eq!(tc.install_steps, vec!["apt-get install -y foo"]);
        assert!(!tc.builtin);
    }

    #[test]
    fn set_image_status_updates() {
        let conn = open_db(":memory:").unwrap();
        set_image_status(&conn, "node", "pulling").unwrap();
        let tc = get_toolchain(&conn, "node").unwrap().unwrap();
        assert_eq!(tc.image_status, "pulling");
    }

    #[test]
    fn append_build_log_accumulates() {
        let conn = open_db(":memory:").unwrap();
        insert_toolchain(&conn, &make_tc("test")).unwrap();
        append_build_log(&conn, "test", "Step 1/3").unwrap();
        append_build_log(&conn, "test", "Step 2/3").unwrap();
        let tc = get_toolchain(&conn, "test").unwrap().unwrap();
        let log = tc.build_log.unwrap();
        assert!(log.contains("Step 1/3"));
        assert!(log.contains("Step 2/3"));
    }

    #[test]
    fn delete_custom_toolchain() {
        let conn = open_db(":memory:").unwrap();
        insert_toolchain(&conn, &make_tc("test")).unwrap();
        delete_toolchain(&conn, "test").unwrap();
        assert!(get_toolchain(&conn, "test").unwrap().is_none());
    }

    #[test]
    fn update_install_steps_persists() {
        let conn = open_db(":memory:").unwrap();
        insert_toolchain(&conn, &make_tc("test")).unwrap();
        update_install_steps(&conn, "test", &["apt-get install -y bar".into()]).unwrap();
        let tc = get_toolchain(&conn, "test").unwrap().unwrap();
        assert_eq!(tc.install_steps, vec!["apt-get install -y bar"]);
    }
}
