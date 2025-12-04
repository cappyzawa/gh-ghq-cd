use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Result, bail};

pub trait ShellExecutor {
    fn exec(&self, shell: &str) -> Result<()>;
}

pub struct SystemShellExecutor;

impl ShellExecutor for SystemShellExecutor {
    fn exec(&self, shell: &str) -> Result<()> {
        // exec replaces the current process
        let err = Command::new(shell).exec();

        // If we get here, exec failed
        bail!("failed to exec {}: {}", shell, err);
    }
}
