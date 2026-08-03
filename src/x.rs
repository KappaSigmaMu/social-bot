use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct XBuffer {
    http: Client,
    url: String,
    api_key: String,
    channel_id: String,
}

impl XBuffer {
    pub fn new(url: String, api_key: String, channel_id: String) -> Self {
        Self {
            http: Client::new(),
            url,
            api_key,
            channel_id,
        }
    }

    pub fn payload(channel_id: &str, text: &str) -> Value {
        let text = serde_json::to_string(text).expect("serializing text cannot fail");
        let channel_id =
            serde_json::to_string(channel_id).expect("serializing channel id cannot fail");
        json!({
            "query": format!(
                "mutation CreatePost {{ createPost(input: {{ text: {text}, channelId: {channel_id}, \
                 schedulingType: automatic, mode: shareNow }}) {{ ... on PostActionSuccess {{ post {{ id }} }} \
                 ... on MutationError {{ message }} }} }}"
            )
        })
    }

    pub async fn post(&self, text: &str) -> Result<()> {
        let response = self
            .http
            .post(&self.url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&Self::payload(&self.channel_id, text))
            .send()
            .await
            .context("posting to Buffer")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unreadable response body".to_owned());
        if !status.is_success() {
            anyhow::bail!("Buffer returned {status}: {body}");
        }
        let value: Value =
            serde_json::from_str(&body).context("Buffer returned non-JSON response")?;
        if let Some(message) = value["data"]["createPost"]["message"].as_str() {
            anyhow::bail!("Buffer createPost failed: {message}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_share_now_mutation() {
        let payload = XBuffer::payload("channel-1", "hello world");
        let query = payload["query"].as_str().unwrap();
        assert!(query.contains("mode: shareNow"));
        assert!(query.contains("schedulingType: automatic"));
        assert!(query.contains("channelId: \"channel-1\""));
        assert!(query.contains("text: \"hello world\""));
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_text() {
        let payload = XBuffer::payload("channel-1", "say \"hi\" \\ done");
        let query = payload["query"].as_str().unwrap();
        assert!(query.contains("text: \"say \\\"hi\\\" \\\\ done\""));
    }
}
