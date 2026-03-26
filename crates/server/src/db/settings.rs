use anyhow::Context;
use crabbit_common::{ClaudeSettings, UpdateClaudeSettingsRequest};
use rusqlite::{params, Connection};

pub fn get_claude_settings(conn: &Connection) -> anyhow::Result<ClaudeSettings> {
    conn.query_row(
        "SELECT model, effort_level, max_budget_usd, usage_limit_pct, system_prompt_append, allow_browser_automation, extra_flags
         FROM claude_settings WHERE id = 1",
        [],
        |row| {
            let extra_flags_json: Option<String> = row.get(6)?;
            let extra_flags = extra_flags_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            Ok(ClaudeSettings {
                model: row.get(0)?,
                effort_level: row.get(1)?,
                max_budget_usd: row.get(2)?,
                usage_limit_pct: row.get(3)?,
                system_prompt_append: row.get(4)?,
                allow_browser_automation: row.get::<_, i64>(5)? != 0,
                extra_flags,
            })
        },
    )
    .context("get_claude_settings")
}

pub fn update_claude_settings(
    conn: &Connection,
    req: &UpdateClaudeSettingsRequest,
) -> anyhow::Result<()> {
    let extra_flags_json = req.extra_flags.as_ref().map(|f| serde_json::to_string(f).unwrap());
    conn.execute(
        "UPDATE claude_settings SET
            model                    = COALESCE(?1, model),
            effort_level             = COALESCE(?2, effort_level),
            max_budget_usd           = COALESCE(?3, max_budget_usd),
            usage_limit_pct          = COALESCE(?4, usage_limit_pct),
            system_prompt_append     = COALESCE(?5, system_prompt_append),
            allow_browser_automation = COALESCE(?6, allow_browser_automation),
            extra_flags              = COALESCE(?7, extra_flags)
         WHERE id = 1",
        params![
            req.model, req.effort_level, req.max_budget_usd, req.usage_limit_pct,
            req.system_prompt_append,
            req.allow_browser_automation.map(|b| b as i64),
            extra_flags_json
        ],
    )
    .context("update_claude_settings")?;
    Ok(())
}
