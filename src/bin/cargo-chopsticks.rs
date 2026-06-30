use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const DEFAULT_BLOCK: &str = "18482417";
const BASE_CONFIG: &str = "tests/e2e/kusama.yml";
const DB_FILES: [&str; 3] = ["db.sqlite", "db.sqlite-shm", "db.sqlite-wal"];

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
    let (clean, chopsticks_args) = parse_args(env::args().skip(1));
    if clean {
        clean_db()?;
    }

    let config_path = config_path(!clean)?;
    let block = env::var("KUSAMA_BLOCK_NUMBER")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BLOCK.to_owned());
    let status = Command::new("npx")
        .arg("@acala-network/chopsticks@1.4.2")
        .arg(format!("--config={}", config_path.display()))
        .arg("--host=0.0.0.0")
        .arg("--port=8000")
        .arg("--build-block-mode=Instant")
        .args(chopsticks_args)
        .env("KUSAMA_BLOCK_NUMBER", block)
        .status()
        .context("running Chopsticks through npx")?;

    if !status.success() {
        anyhow::bail!("Chopsticks exited with {status}");
    }

    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> (bool, Vec<String>) {
    let mut clean = false;
    let mut chopsticks_args = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--" => {}
            "--clean" => clean = true,
            _ => chopsticks_args.push(arg),
        }
    }

    (clean, chopsticks_args)
}

fn config_path(resume: bool) -> Result<PathBuf> {
    if !resume {
        return Ok(PathBuf::from(BASE_CONFIG));
    }

    let config =
        fs::read_to_string(BASE_CONFIG).with_context(|| format!("reading {BASE_CONFIG}"))?;
    let path = env::temp_dir().join(format!("element-bot-chopsticks-{}.yml", std::process::id()));
    fs::write(&path, format!("{config}\nresume: true\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

fn clean_db() -> Result<()> {
    for path in DB_FILES {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("removing {path}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn resumes_by_default() {
        let (clean, args) = parse_args(Vec::<String>::new());

        assert!(!clean);
        assert!(args.is_empty());
    }

    #[test]
    fn clean_disables_resume() {
        let (clean, args) = parse_args(["--clean".to_owned()]);

        assert!(clean);
        assert!(args.is_empty());
    }

    #[test]
    fn forwards_extra_args_and_ignores_separator() {
        let (clean, args) = parse_args(["--".to_owned(), "--rpc-timeout=120000".to_owned()]);

        assert!(!clean);
        assert_eq!(args, ["--rpc-timeout=120000"]);
    }
}
