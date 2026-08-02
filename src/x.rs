use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct XWebhook {
    http: Client,
    url: String,
}

impl XWebhook {
    pub fn new(url: String) -> Self {
        Self {
            http: Client::new(),
            url,
        }
    }

    pub fn payload(text: &str) -> Value {
        serde_json::json!({ "text": text })
    }

    pub async fn post(&self, text: &str) -> Result<()> {
        let response = self
            .http
            .post(&self.url)
            .json(&Self::payload(text))
            .send()
            .await
            .context("posting to X webhook")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unreadable response body".to_owned());
            anyhow::bail!("X webhook returned {status}: {body}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_payload_with_plain_text() {
        assert_eq!(
            XWebhook::payload("hello world"),
            serde_json::json!({ "text": "hello world" })
        );
    }
}
