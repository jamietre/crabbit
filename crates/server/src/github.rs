use anyhow::Context;
use crabbit_common::{Repo, SyncResult};
use rusqlite::Connection;
use serde::Deserialize;

use crate::db::sync as sync_db;

#[derive(Debug, Clone)]
pub struct GitHubIssue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub html_url: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitHubClient {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

impl GitHubClient {
    pub fn new(token: String, base_url: String) -> Self {
        Self {
            token,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_token(token: String) -> Self {
        Self::new(token, "https://api.github.com".into())
    }

    pub async fn list_open_issues(
        &self,
        owner: &str,
        repo: &str,
        label_filter: Option<&str>,
    ) -> anyhow::Result<Vec<GitHubIssue>> {
        let url = format!("{}/repos/{}/{}/issues", self.base_url, owner, repo);
        let mut req = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "crabbit/1.0")
            .header("Accept", "application/vnd.github+json")
            .query(&[("state", "open"), ("per_page", "100")]);

        if let Some(label) = label_filter {
            req = req.query(&[("labels", label)]);
        }

        let resp = req.send().await.context("github request failed")?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("GITHUB_AUTH_EXPIRED"));
        }
        let items: Vec<RawIssue> = resp
            .error_for_status()
            .context("github API error")?
            .json()
            .await
            .context("github response parse error")?;

        let issues = items
            .into_iter()
            .filter(|i| {
                if let Some(label) = label_filter {
                    i.labels.iter().any(|l| l.name == label)
                } else {
                    true
                }
            })
            .map(|i| GitHubIssue {
                number: i.number,
                title: i.title.clone(),
                body: i.body.clone().unwrap_or_default(),
                html_url: i.html_url.clone(),
                labels: i.labels.iter().map(|l| l.name.clone()).collect(),
            })
            .collect();

        Ok(issues)
    }

    /// List filenames at the root of a repo — used for toolchain auto-detection.
    /// Returns an empty vec (not an error) if the repo is private and the token lacks access.
    pub async fn list_root_files(&self, owner: &str, repo: &str) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/repos/{}/{}/contents/", self.base_url, owner, repo);
        let resp = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "crabbit/1.0")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("github request failed")?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let items: Vec<serde_json::Value> = resp.json().await.unwrap_or_default();
        Ok(items.iter()
            .filter_map(|f| f["name"].as_str().map(String::from))
            .collect())
    }

    /// Fetch all open issues for a repo, applying labels_require and labels_ignore filters.
    pub async fn list_issues_for_sync(
        &self,
        owner: &str,
        repo: &str,
        labels_require: &[String],
        labels_ignore: &[String],
    ) -> anyhow::Result<Vec<GitHubIssue>> {
        // Pass the first require label to the API to reduce result set (API supports one label)
        let api_label = labels_require.first().map(|s| s.as_str());
        let all = self.list_open_issues(owner, repo, api_label).await?;

        Ok(all.into_iter().filter(|issue| {
            // Must have at least one require label (if configured)
            if !labels_require.is_empty()
                && !labels_require.iter().any(|req| issue.labels.contains(req)) {
                return false;
            }
            // Must not have any ignore label
            if labels_ignore.iter().any(|ign| issue.labels.contains(ign)) {
                return false;
            }
            true
        }).collect())
    }
}

/// Fetch issues from GitHub for a repo (async part of sync — no DB access).
pub async fn fetch_issues_for_sync(
    client: &GitHubClient,
    repo: &Repo,
) -> anyhow::Result<Vec<GitHubIssue>> {
    client
        .list_issues_for_sync(&repo.owner, &repo.name, &repo.labels_require, &repo.labels_ignore)
        .await
}

/// Write already-fetched issues to the local task queue (sync, takes &Connection).
pub fn sync_issues_to_db(
    conn: &Connection,
    repo: &Repo,
    issues: &[GitHubIssue],
) -> anyhow::Result<SyncResult> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let open_numbers: Vec<i64> = issues.iter().map(|i| i.number).collect();
    let mut result = SyncResult { created: 0, updated: 0, closed: 0 };

    for issue in issues {
        let labels_json = serde_json::to_string(&issue.labels).unwrap_or_else(|_| "[]".into());
        let is_prioritized = repo.labels_prioritize.iter().any(|p| issue.labels.contains(p));

        let (created, updated) = sync_db::upsert_issue_as_task(
            conn,
            repo.id,
            issue.number,
            &issue.title,
            &issue.html_url,
            &issue.body,
            &labels_json,
            is_prioritized,
            now,
        )?;
        if created { result.created += 1; }
        if updated { result.updated += 1; }
    }

    result.closed = sync_db::close_stale_queued_tasks(conn, repo.id, &open_numbers, now)?;
    Ok(result)
}

#[derive(Deserialize)]
struct RawIssue {
    number: i64,
    title: String,
    body: Option<String>,
    html_url: String,
    labels: Vec<RawLabel>,
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    #[tokio::test]
    async fn list_open_issues_returns_parsed_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/issues"))
            .and(header("Authorization", "Bearer ghp_test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "number": 42,
                    "title": "Fix the bug",
                    "body": "It is broken",
                    "html_url": "https://github.com/acme/api/issues/42",
                    "labels": []
                }
            ])))
            .mount(&server)
            .await;

        let client = GitHubClient::new("ghp_test".into(), server.uri());
        let issues = client.list_open_issues("acme", "api", None).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 42);
        assert_eq!(issues[0].title, "Fix the bug");
    }

    #[tokio::test]
    async fn list_open_issues_returns_auth_expired_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/issues"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = GitHubClient::new("bad_token".into(), server.uri());
        let err = client.list_open_issues("acme", "api", None).await.unwrap_err();
        assert!(err.to_string().contains("GITHUB_AUTH_EXPIRED"));
    }

    #[tokio::test]
    async fn list_open_issues_filters_by_label() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"number": 1, "title": "t1", "body": "b", "html_url": "u",
                 "labels": [{"name": "crabbit"}]},
                {"number": 2, "title": "t2", "body": "b", "html_url": "u",
                 "labels": [{"name": "bug"}]}
            ])))
            .mount(&server)
            .await;

        let client = GitHubClient::new("ghp_test".into(), server.uri());
        let issues = client.list_open_issues("acme", "api", Some("crabbit")).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 1);
    }
}
