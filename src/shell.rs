use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Result, bail};

/// Execute shell, replacing the current process
pub fn exec(shell: &str) -> Result<()> {
    let err = Command::new(shell).exec();

    // If we get here, exec failed
    bail!("failed to exec {}: {}", shell, err);
}
