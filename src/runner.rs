use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", cmd))?;

    if !output.status.success() {
        bail!("{} failed", cmd);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
