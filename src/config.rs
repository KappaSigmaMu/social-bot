use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub matrix_room: String,
    pub matrix_token: String,
    pub matrix_homeserver: String,
    pub matrix_user_id: String,
    pub rpc_url: String,
    pub db_path: String,
    pub prefix: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        Ok(Self {
            matrix_room: required("MATRIX_ROOM")?,
            matrix_token: required("MATRIX_TOKEN")?,
            matrix_homeserver: env::var("MATRIX_HOMESERVER")
                .unwrap_or_else(|_| "https://matrix.org".to_owned()),
            matrix_user_id: env::var("MATRIX_USER_ID")
                .unwrap_or_else(|_| "@societybot:matrix.org".to_owned()),
            rpc_url: env::var("RPC_URL")
                .unwrap_or_else(|_| "wss://kusama-rpc.polkadot.io/".to_owned()),
            db_path: env::var("DB_PATH").unwrap_or_else(|_| "./society_overrides.db".to_owned()),
            prefix: env::var("PREFIX").unwrap_or_else(|_| "!".to_owned()),
        })
    }
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} must be set"))
}
