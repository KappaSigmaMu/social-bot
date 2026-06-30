use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SUBMODULE_DIR: &str = "custom-kusama-runtime";
pub const SUBMODULE_BRANCH: &str = "customized-society-pallet";
pub const WASM_OUTPUT: &str = "custom-kusama-runtime.wasm";
const WASM_BUILD_REL: &str =
    "target/release/wbuild/staging-kusama-runtime/staging_kusama_runtime.wasm";

pub fn ensure_custom_runtime_wasm() -> Result<PathBuf> {
    let wasm_path = PathBuf::from(WASM_OUTPUT);
    if wasm_path.is_file() {
        return Ok(wasm_path);
    }
    build_custom_runtime_wasm()?;
    if !wasm_path.is_file() {
        bail!("custom runtime build finished but {WASM_OUTPUT} was not created");
    }
    Ok(wasm_path)
}

pub fn build_custom_runtime_wasm() -> Result<()> {
    let submodule = PathBuf::from(SUBMODULE_DIR);
    if !submodule.is_dir() {
        bail!(
            "missing {SUBMODULE_DIR}; run `git submodule update --init --recursive` first"
        );
    }

    ensure_submodule_branch(&submodule)?;

    let runtime_dir = submodule.join("relay/kusama");
    if !runtime_dir.is_dir() {
        bail!("missing {} in {SUBMODULE_DIR}", runtime_dir.display());
    }

    eprintln!(
        "Building custom Kusama runtime in {} (this can take several minutes)...",
        runtime_dir.display()
    );

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&runtime_dir)
        .status()
        .with_context(|| format!("running cargo build in {}", runtime_dir.display()))?;

    if !status.success() {
        bail!("custom runtime build failed with {status}");
    }

    let built_wasm = runtime_dir.join(WASM_BUILD_REL);
    fs::copy(&built_wasm, WASM_OUTPUT).with_context(|| {
        format!(
            "copying {} to {WASM_OUTPUT}",
            built_wasm.display()
        )
    })?;

    eprintln!("Wrote {WASM_OUTPUT}");
    Ok(())
}

pub fn apply_wasm_override(config: &str) -> String {
    if config.lines().any(|line| {
        let line = line.trim();
        line.starts_with("wasm-override:")
    }) {
        return config.replace(
            "# wasm-override: ./custom-kusama-runtime.wasm",
            "wasm-override: ./custom-kusama-runtime.wasm",
        );
    }

    let mut updated = config.replace(
        "# wasm-override: ./custom-kusama-runtime.wasm",
        "wasm-override: ./custom-kusama-runtime.wasm",
    );
    if !updated.contains("wasm-override:") {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str("wasm-override: ./custom-kusama-runtime.wasm\n");
    }
    updated
}

fn ensure_submodule_branch(submodule: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["-C", &submodule.display().to_string(), "rev-parse", "--is-inside-work-tree"])
        .output()
        .with_context(|| format!("checking git repo at {}", submodule.display()))?;

    if !status.status.success() {
        bail!("{} is not a git checkout", submodule.display());
    }

    let checkout = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "checkout",
            SUBMODULE_BRANCH,
        ])
        .status()
        .with_context(|| format!("checking out {SUBMODULE_BRANCH} in {}", submodule.display()))?;

    if !checkout.success() {
        bail!(
            "failed to checkout branch {SUBMODULE_BRANCH} in {}; run `git submodule update --init --recursive`",
            submodule.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::apply_wasm_override;

    #[test]
    fn uncomments_wasm_override_in_config() {
        let config = "db: ./db.sqlite\n# wasm-override: ./custom-kusama-runtime.wasm\n";
        let updated = apply_wasm_override(config);
        assert!(updated.contains("wasm-override: ./custom-kusama-runtime.wasm"));
        assert!(!updated.contains("# wasm-override"));
    }

    #[test]
    fn keeps_existing_wasm_override() {
        let config = "wasm-override: ./custom-kusama-runtime.wasm\n";
        assert_eq!(apply_wasm_override(config), config);
    }
}
