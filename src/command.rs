use anyhow::{Context, Result, bail};
use std::process::Command;
use which::which;

pub trait CommandChecker {
    fn check(&self, cmd: &str) -> Result<()>;
}

pub trait CommandRunner {
    fn run(&self, cmd: &str, args: &[&str]) -> Result<String>;
}

pub struct SystemCommandChecker;

impl CommandChecker for SystemCommandChecker {
    fn check(&self, cmd: &str) -> Result<()> {
        which(cmd).with_context(|| format!("{} not found on the system", cmd))?;
        Ok(())
    }
}

pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, cmd: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {}", cmd))?;

        if !output.status.success() {
            bail!("{} failed", cmd);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
