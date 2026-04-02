use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use crabbit_common::{CreateToolchainRequest, GenerateStepsRequest, GenerateStepsResponse, Toolchain};
use crate::{db::toolchains as db, error::{ApiError, ApiResult}, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/generate-steps", post(generate_steps))
        .route("/:name", delete(remove))
        .route("/:name/pull", post(pull))
        .route("/:name/build", post(build))
}

async fn list(State(s): State<AppState>) -> ApiResult<Vec<Toolchain>> {
    Ok(Json(s.with_db(db::list_toolchains)?))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateToolchainRequest>,
) -> Result<(StatusCode, Json<Toolchain>), ApiError> {
    let now = unix_now();
    let image = format!("crabbit-{}:local", req.name);
    let tc = Toolchain {
        name: req.name.clone(),
        display_name: req.display_name,
        image,
        image_status: "pending".into(),
        builtin: false,
        install_steps: req.install_steps,
        detection_markers: vec![],
        build_log: None,
        created_at: now,
    };
    s.with_db(|c| db::insert_toolchain(c, &tc))?;
    let created = s.with_db(|c| db::get_toolchain(c, &req.name))?.unwrap();
    Ok((StatusCode::CREATED, Json(created)))
}

async fn remove(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let tc = s.with_db(|c| db::get_toolchain(c, &name))?
        .ok_or_else(|| ApiError::NotFound(format!("toolchain '{name}'")))?;
    if tc.builtin {
        return Err(ApiError::BadRequest("cannot delete a built-in toolchain".into()));
    }
    s.with_db(|c| db::delete_toolchain(c, &name))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn pull(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let tc = s.with_db(|c| db::get_toolchain(c, &name))?
        .ok_or_else(|| ApiError::NotFound(format!("toolchain '{name}'")))?;
    if !tc.builtin {
        return Err(ApiError::BadRequest("use /build for custom toolchains".into()));
    }

    s.with_db(|c| db::set_image_status(c, &name, "pulling"))?;

    tokio::spawn(async move {
        let ok = tokio::process::Command::new("docker")
            .args(["pull", &tc.image])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        let status = if ok { "available" } else { "pull_failed" };
        let _ = s.with_db(|c| db::set_image_status(c, &name, status));
    });

    Ok(StatusCode::ACCEPTED)
}

async fn build(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let tc = s.with_db(|c| db::get_toolchain(c, &name))?
        .ok_or_else(|| ApiError::NotFound(format!("toolchain '{name}'")))?;
    if tc.builtin {
        return Err(ApiError::BadRequest("use /pull for built-in toolchains".into()));
    }
    if tc.install_steps.is_empty() {
        return Err(ApiError::BadRequest("no install_steps defined".into()));
    }

    s.with_db(|c| {
        db::clear_build_log(c, &name)?;
        db::set_image_status(c, &name, "building")
    })?;

    tokio::spawn(async move {
        let tmpdir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("failed to create tempdir for docker build: {e}");
                let _ = s.with_db(|c| db::set_image_status(c, &name, "build_failed"));
                return;
            }
        };

        let steps = tc.install_steps.iter()
            .map(|cmd| format!("RUN {cmd}"))
            .collect::<Vec<_>>()
            .join("\n");
        let dockerfile = format!("FROM ghcr.io/jamietre/crabbit-base:latest\n{steps}\n");
        if std::fs::write(tmpdir.path().join("Dockerfile"), dockerfile).is_err() {
            let _ = s.with_db(|c| db::set_image_status(c, &name, "build_failed"));
            return;
        }

        let image_tag = format!("crabbit-{}:local", name);
        let mut child = match tokio::process::Command::new("docker")
            .args(["build", "-t", &image_tag, tmpdir.path().to_str().unwrap_or(".")])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("docker build spawn failed: {e}");
                let _ = s.with_db(|c| db::set_image_status(c, &name, "build_failed"));
                return;
            }
        };

        // Stream stderr (docker build output) to DB line by line
        use tokio::io::{AsyncBufReadExt, BufReader};
        if let Some(stderr) = child.stderr.take() {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = s.with_db(|c| db::append_build_log(c, &name, &line));
            }
        }

        let status = if child.wait().await.map(|s| s.success()).unwrap_or(false) {
            "available"
        } else {
            "build_failed"
        };
        let _ = s.with_db(|c| db::set_image_status(c, &name, status));
    });

    Ok(StatusCode::ACCEPTED)
}

async fn generate_steps(
    Json(req): Json<GenerateStepsRequest>,
) -> ApiResult<GenerateStepsResponse> {
    let prompt = format!(
        "Generate a list of shell commands to install the following on Ubuntu 24.04: {}\n\n\
         Output ONLY a JSON array of strings, each being one shell command to RUN in a Dockerfile. \
         No markdown, no explanation, no code fences. \
         Example: [\"apt-get install -y foo\", \"npm install -g bar\"]",
        req.description
    );

    let output = tokio::process::Command::new("claude")
        .args(["--dangerously-skip-permissions", "-p", &prompt])
        .output()
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to invoke claude: {e}")))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let json_start = text.find('[').unwrap_or(0);
    let json_end = text.rfind(']').map(|i| i + 1).unwrap_or(text.len());
    let steps: Vec<String> = serde_json::from_str(&text[json_start..json_end])
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to parse claude output: {e}")))?;

    Ok(Json(GenerateStepsResponse { steps }))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use crabbit_common::Toolchain;
    use crate::routes::tests::test_server;

    #[tokio::test]
    async fn list_returns_seeded_toolchains() {
        let server = test_server();
        let r = server.get("/api/v1/toolchains").await;
        assert_eq!(r.status_code(), 200);
        let tcs: Vec<Toolchain> = r.json();
        assert!(tcs.iter().any(|t| t.name == "node"));
        assert!(tcs.iter().any(|t| t.name == "rust"));
    }

    #[tokio::test]
    async fn create_custom_toolchain() {
        let server = test_server();
        let r = server.post("/api/v1/toolchains")
            .json(&serde_json::json!({
                "name": "elixir",
                "display_name": "Elixir",
                "install_steps": ["apt-get install -y elixir"]
            }))
            .await;
        assert_eq!(r.status_code(), 201);
        let tc: Toolchain = r.json();
        assert_eq!(tc.name, "elixir");
        assert!(!tc.builtin);
        assert_eq!(tc.image, "crabbit-elixir:local");
    }

    #[tokio::test]
    async fn delete_builtin_fails() {
        let server = test_server();
        let r = server.delete("/api/v1/toolchains/node").await;
        assert_eq!(r.status_code(), 400);
    }

    #[tokio::test]
    async fn delete_custom_toolchain() {
        let server = test_server();
        server.post("/api/v1/toolchains")
            .json(&serde_json::json!({"name": "del-test", "display_name": "Del", "install_steps": []}))
            .await;
        let r = server.delete("/api/v1/toolchains/del-test").await;
        assert_eq!(r.status_code(), 204);
    }

    #[tokio::test]
    async fn delete_unknown_returns_404() {
        let server = test_server();
        let r = server.delete("/api/v1/toolchains/nonexistent").await;
        assert_eq!(r.status_code(), 404);
    }

    #[tokio::test]
    async fn build_builtin_returns_400() {
        let server = test_server();
        let r = server.post("/api/v1/toolchains/node/build").await;
        assert_eq!(r.status_code(), 400);
    }

    #[tokio::test]
    async fn pull_custom_returns_400() {
        let server = test_server();
        server.post("/api/v1/toolchains")
            .json(&serde_json::json!({"name": "custom", "display_name": "Custom", "install_steps": ["echo hi"]}))
            .await;
        let r = server.post("/api/v1/toolchains/custom/pull").await;
        assert_eq!(r.status_code(), 400);
    }
}
