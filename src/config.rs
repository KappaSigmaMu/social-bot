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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn set_env(name: &str, value: &str) {
        unsafe {
            env::set_var(name, value);
        }
    }

    fn remove_env(name: &str) {
        unsafe {
            env::remove_var(name);
        }
    }

    #[test]
    fn loads_required_values_and_defaults() {
        let _guard = env_lock().lock().unwrap();
        clear_env();
        set_env("MATRIX_ROOM", "!room:e2e.local");
        set_env("MATRIX_TOKEN", "token");

        let config = Config::from_env().unwrap();
        assert_eq!(config.matrix_room, "!room:e2e.local");
        assert_eq!(config.matrix_token, "token");
        assert_eq!(config.matrix_homeserver, "https://matrix.org");
        assert_eq!(config.matrix_user_id, "@societybot:matrix.org");
        assert_eq!(config.rpc_url, "wss://kusama-rpc.polkadot.io/");
        assert_eq!(config.db_path, "./society_overrides.db");
        assert_eq!(config.prefix, "!");

        clear_env();
    }

    #[test]
    fn loads_optional_overrides() {
        let _guard = env_lock().lock().unwrap();
        clear_env();
        set_env("MATRIX_ROOM", "!room:e2e.local");
        set_env("MATRIX_TOKEN", "token");
        set_env("MATRIX_HOMESERVER", "http://matrix:8008");
        set_env("MATRIX_USER_ID", "@bot:e2e.local");
        set_env("RPC_URL", "ws://chopsticks:8000");
        set_env("DB_PATH", "/tmp/society.db");
        set_env("PREFIX", "?");

        let config = Config::from_env().unwrap();
        assert_eq!(config.matrix_homeserver, "http://matrix:8008");
        assert_eq!(config.matrix_user_id, "@bot:e2e.local");
        assert_eq!(config.rpc_url, "ws://chopsticks:8000");
        assert_eq!(config.db_path, "/tmp/society.db");
        assert_eq!(config.prefix, "?");

        clear_env();
    }

    #[test]
    fn requires_matrix_room_and_token() {
        let _guard = env_lock().lock().unwrap();
        clear_env();
        assert!(
            Config::from_env()
                .unwrap_err()
                .to_string()
                .contains("MATRIX_ROOM")
        );

        set_env("MATRIX_ROOM", "!room:e2e.local");
        assert!(
            Config::from_env()
                .unwrap_err()
                .to_string()
                .contains("MATRIX_TOKEN")
        );

        clear_env();
    }

    fn clear_env() {
        for name in [
            "MATRIX_ROOM",
            "MATRIX_TOKEN",
            "MATRIX_HOMESERVER",
            "MATRIX_USER_ID",
            "RPC_URL",
            "DB_PATH",
            "PREFIX",
        ] {
            remove_env(name);
        }
    }
}
