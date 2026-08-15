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
            // The only description of the failure is on stderr, which
            // Command::output() captures instead of forwarding to the terminal.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if stderr.is_empty() {
                bail!("{} failed", cmd);
            }
            bail!("{} failed: {}", cmd, stderr);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_reports_stderr_of_a_failing_command() {
        let err = SystemCommandRunner
            .run("sh", &["-c", "echo boom >&2; exit 1"])
            .unwrap_err();

        assert_eq!(err.to_string(), "sh failed: boom");
    }

    #[test]
    fn test_run_reports_a_failing_command_without_stderr() {
        let err = SystemCommandRunner
            .run("sh", &["-c", "exit 1"])
            .unwrap_err();

        assert_eq!(err.to_string(), "sh failed");
    }
}
