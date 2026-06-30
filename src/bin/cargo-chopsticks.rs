use anyhow::{Context, Result};
use element_bot::custom_runtime::{apply_wasm_override, ensure_custom_runtime_wasm};
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
    let options = parse_args(env::args().skip(1));
    if options.clean {
        clean_db()?;
    }
    if options.custom {
        ensure_custom_runtime_wasm()?;
    }

    let config_path = config_path(!options.clean, options.custom)?;
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
        .args(options.chopsticks_args)
        .env("KUSAMA_BLOCK_NUMBER", block)
        .status()
        .context("running Chopsticks through npx")?;

    if !status.success() {
        anyhow::bail!("Chopsticks exited with {status}");
    }

    Ok(())
}

struct ChopsticksOptions {
    clean: bool,
    custom: bool,
    chopsticks_args: Vec<String>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> ChopsticksOptions {
    let mut clean = false;
    let mut custom = false;
    let mut chopsticks_args = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--" => {}
            "--clean" => clean = true,
            "--custom" => custom = true,
            _ => chopsticks_args.push(arg),
        }
    }

    ChopsticksOptions {
        clean,
        custom,
        chopsticks_args,
    }
}

fn config_path(resume: bool, custom: bool) -> Result<PathBuf> {
    if !resume && !custom {
        return Ok(PathBuf::from(BASE_CONFIG));
    }

    let mut config = fs::read_to_string(BASE_CONFIG).with_context(|| format!("reading {BASE_CONFIG}"))?;
    if custom {
        config = apply_wasm_override(&config);
    }
    if resume {
        config.push_str("\nresume: true\n");
    }

    let path = env::temp_dir().join(format!("element-bot-chopsticks-{}.yml", std::process::id()));
    fs::write(&path, config).with_context(|| format!("writing {}", path.display()))?;
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
        let options = parse_args(Vec::<String>::new());

        assert!(!options.clean);
        assert!(!options.custom);
        assert!(options.chopsticks_args.is_empty());
    }

    #[test]
    fn clean_disables_resume() {
        let options = parse_args(["--clean".to_owned()]);

        assert!(options.clean);
        assert!(!options.custom);
        assert!(options.chopsticks_args.is_empty());
    }

    #[test]
    fn enables_custom_runtime() {
        let options = parse_args(["--custom".to_owned()]);

        assert!(options.custom);
        assert!(!options.clean);
    }

    #[test]
    fn forwards_extra_args_and_ignores_separator() {
        let options = parse_args(["--".to_owned(), "--rpc-timeout=120000".to_owned()]);

        assert!(!options.clean);
        assert_eq!(options.chopsticks_args, ["--rpc-timeout=120000"]);
    }
}
