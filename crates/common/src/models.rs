use serde::{Deserialize, Serialize};

// ── Repos ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub enabled: bool,
    pub label_filter: Option<String>,
    pub labels_require: Vec<String>,
    pub labels_ignore: Vec<String>,
    pub labels_prioritize: Vec<String>,
    pub completion_prompt: Option<String>,
    pub toolchain: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRepoRequest {
    pub owner: String,
    pub name: String,
    pub label_filter: Option<String>,
    pub labels_require: Option<Vec<String>>,
    pub labels_ignore: Option<Vec<String>>,
    pub labels_prioritize: Option<Vec<String>>,
    pub completion_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRepoRequest {
    pub enabled: Option<bool>,
    pub label_filter: Option<Option<String>>, // Some(None) = clear it
    pub labels_require: Option<Vec<String>>,
    pub labels_ignore: Option<Vec<String>>,
    pub labels_prioritize: Option<Vec<String>>,
    pub completion_prompt: Option<Option<String>>, // Some(None) = clear it
    pub toolchain: Option<Option<String>>,         // Some(None) = clear it
}

// ── Tasks ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Pending,
    InProgress,
    Retrying,
    PrCreated,
    NeedsHuman,
    Failed,
    Skipped,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::PrCreated | TaskStatus::Failed | TaskStatus::Skipped)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap();
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(&format!("\"{}\"", s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
    pub status: TaskStatus,
    pub task_type: String,
    pub issue_labels: Vec<String>,
    pub is_prioritized: bool,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub error_message: Option<String>,
    pub claude_session_id: Option<String>,
    pub retry_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub repo_id: i64,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub status: Option<TaskStatus>,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub error_message: Option<String>,
    pub claude_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub repo_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Task Events ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskEventRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
}

// ── Task With Events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskWithEvents {
    #[serde(flatten)]
    pub task: Task,
    pub events: Vec<TaskEvent>,
}

// ── Agent State ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Running,
    Sleeping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub status: AgentStatus,
    pub wake_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub current_task_id: Option<i64>,
    pub usage_note: Option<String>,
    pub usage_pct_7d: Option<f64>,
    pub usage_pct_5h: Option<f64>,
    pub usage_reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentStateRequest {
    pub status: Option<AgentStatus>,
    pub wake_at: Option<i64>,
    pub current_task_id: Option<i64>,
    pub usage_note: Option<String>,
    pub usage_pct_7d: Option<f64>,
    pub usage_pct_5h: Option<f64>,
    pub usage_reset_at: Option<i64>,
}

// ── Claude Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthStatus {
    pub configured: bool,
    pub updated_at: Option<i64>,
    pub check: ClaudeAuthCheckStatus,
}

/// Result of the last explicit auth verification against the Anthropic API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthCheckStatus {
    /// "ok" | "expired" | "unknown"
    pub status: String,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushClaudeAuthRequest {
    /// Full contents of the .credentials.json file (includes refresh token).
    pub credentials_json: String,
    pub sync_secret: String,
}

// ── GitHub Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAuthStatus {
    pub connected: bool,
    pub github_login: Option<String>,
    pub token_scopes: Option<String>,
    pub connected_at: Option<i64>,
    /// Only populated when `?include_token=true` is passed by authenticated callers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

// ── Claude Settings ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSettings {
    pub model: String,
    pub effort_level: String,
    pub max_budget_usd: Option<f64>,
    pub usage_limit_pct: Option<f64>,
    pub system_prompt_append: Option<String>,
    pub allow_browser_automation: bool,
    pub extra_flags: Vec<String>,
}

impl Default for ClaudeSettings {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            effort_level: "high".into(),
            max_budget_usd: None,
            usage_limit_pct: None,
            system_prompt_append: None,
            allow_browser_automation: true,
            extra_flags: vec![],
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateClaudeSettingsRequest {
    pub model: Option<String>,
    pub effort_level: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub usage_limit_pct: Option<f64>,
    pub system_prompt_append: Option<String>,
    pub allow_browser_automation: Option<bool>,
    pub extra_flags: Option<Vec<String>>,
}

// ── Prompts ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: i64,
    pub category: String,
    pub label: String,
    pub name: String,
    pub content: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreatePromptRequest {
    pub category: String,
    pub label: Option<String>,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePromptRequest {
    pub category: Option<String>,
    pub label: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
    pub enabled: Option<bool>,
}

// ── Toolchains ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolchain {
    pub name: String,
    pub display_name: String,
    pub image: String,
    pub image_status: String, // "not_pulled"|"pulling"|"available"|"pull_failed"|"pending"|"building"|"build_failed"
    pub builtin: bool,
    pub install_steps: Vec<String>,
    pub detection_markers: Vec<String>,
    pub build_log: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateToolchainRequest {
    pub name: String,
    pub display_name: String,
    pub install_steps: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateStepsRequest {
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct GenerateStepsResponse {
    pub steps: Vec<String>,
}

// ── Next Issue ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextIssueResponse {
    pub task_id: i64,
    pub repo_id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
    pub completion_prompt: Option<String>,
    pub existing_task_id: Option<i64>, // kept for backward compat; same as task_id
}

// ── Sync ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub created: u32,
    pub updated: u32,
    pub closed: u32,
}

impl SyncResult {
    pub fn merge(&mut self, other: SyncResult) {
        self.created += other.created;
        self.updated += other.updated;
        self.closed += other.closed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_roundtrip() {
        let s = serde_json::to_string(&TaskStatus::PrCreated).unwrap();
        assert_eq!(s, "\"pr_created\"");
        let back: TaskStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, TaskStatus::PrCreated);
    }

    #[test]
    fn create_repo_request_roundtrip() {
        let r = CreateRepoRequest {
            owner: "acme".into(),
            name: "myrepo".into(),
            label_filter: Some("crabbit".into()),
            labels_require: Some(vec!["crabbit".into()]),
            labels_ignore: None,
            labels_prioritize: None,
            completion_prompt: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: CreateRepoRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.owner, "acme");
        assert_eq!(back.label_filter, Some("crabbit".into()));
    }

    #[test]
    fn task_status_queued_roundtrip() {
        let s = serde_json::to_string(&TaskStatus::Queued).unwrap();
        assert_eq!(s, "\"queued\"");
        let back: TaskStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, TaskStatus::Queued);
    }
}
