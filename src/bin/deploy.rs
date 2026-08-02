use anyhow::{Context, Result};
use std::env;
use std::process::{Command, ExitCode, Stdio};

const DEFAULT_DEPLOY_DIR: &str = "/opt/social-bot";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let _ = dotenvy::dotenv();

    let host = env::var("DEPLOY_HOST")
        .context("DEPLOY_HOST must be set in .env (for example root@159.223.110.163)")?;
    let deploy_dir = env::var("DEPLOY_DIR").unwrap_or_else(|_| DEFAULT_DEPLOY_DIR.to_owned());
    let remote_cmd = remote_update_command(&deploy_dir);

    eprintln!("Deploying to {host} ({deploy_dir})");

    let status = Command::new("ssh")
        .arg(&host)
        .arg(&remote_cmd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("running ssh {host}"))?;

    if !status.success() {
        anyhow::bail!("deploy failed with {status}");
    }

    Ok(())
}

fn remote_update_command(deploy_dir: &str) -> String {
    format!("cd {deploy_dir} && docker compose pull && docker compose up -d")
}

#[cfg(test)]
mod tests {
    use super::remote_update_command;

    #[test]
    fn builds_remote_update_command() {
        assert_eq!(
            remote_update_command("/opt/social-bot"),
            "cd /opt/social-bot && docker compose pull && docker compose up -d"
        );
    }
}
