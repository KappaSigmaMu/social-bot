use anyhow::{Context, Result, bail};
use social_bot::custom_runtime::ensure_custom_runtime_wasm;
use std::env;
use std::net::TcpStream;
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const CHOPSTICKS_RPC: &str = "127.0.0.1:8000";
const CHOPSTICKS_START_TIMEOUT: Duration = Duration::from_secs(120);

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
    let custom = env::args().skip(1).any(|arg| arg == "--custom");
    let mut chopsticks = None;

    if custom {
        ensure_custom_runtime_wasm()?;
        chopsticks = Some(spawn_chopsticks()?);
        wait_for_chopsticks()?;
        eprintln!("Chopsticks is ready on ws://{CHOPSTICKS_RPC}");
    }

    let status = Command::new("cargo")
        .args(["run", "--bin", "social-bot", "--", "--dev"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running social-bot in dev mode")?;

    if let Some(mut child) = chopsticks {
        let _ = child.kill();
        let _ = child.wait();
    }

    if !status.success() {
        bail!("social-bot exited with {status}");
    }

    Ok(())
}

fn spawn_chopsticks() -> Result<Child> {
    eprintln!("Starting Chopsticks with custom runtime...");
    Command::new("cargo")
        .args(["run", "--bin", "cargo-chopsticks", "--", "--custom"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("starting cargo-chopsticks --custom")
}

fn wait_for_chopsticks() -> Result<()> {
    let deadline = Instant::now() + CHOPSTICKS_START_TIMEOUT;
    while Instant::now() < deadline {
        if TcpStream::connect(CHOPSTICKS_RPC).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    bail!("timed out waiting for Chopsticks on ws://{CHOPSTICKS_RPC}");
}
