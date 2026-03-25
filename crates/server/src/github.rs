use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct GitHubIssue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub html_url: String,
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

        let items: Vec<RawIssue> = req.send().await
            .context("github request failed")?
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
                title: i.title,
                body: i.body.unwrap_or_default(),
                html_url: i.html_url,
            })
            .collect();

        Ok(issues)
    }
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
