use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

pub struct WindowConfig {
    pub name: String,
    pub start_dir: PathBuf,
}

impl WindowConfig {
    pub fn new<S: Into<String>, P: Into<PathBuf>>(name: S, start_dir: P) -> Self {
        Self {
            name: name.into(),
            start_dir: start_dir.into(),
        }
    }
}

pub trait TmuxClient {
    fn new_window(&self, cfg: &WindowConfig) -> Result<()>;
    fn rename_window(&self, name: &str) -> Result<()>;
}

pub struct SystemTmuxClient;
pub struct NoopTmuxClient;

impl TmuxClient for SystemTmuxClient {
    fn new_window(&self, cfg: &WindowConfig) -> Result<()> {
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        let status = Command::new("tmux")
            .args(["new-window", "-n", &cfg.name, "-c", start_dir])
            .status()
            .context("failed to run tmux new-window")?;

        if !status.success() {
            bail!("tmux new-window failed");
        }
        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let _ = Command::new("tmux").args(["rename-window", name]).status();
        Ok(())
    }
}

impl TmuxClient for NoopTmuxClient {
    fn new_window(&self, _: &WindowConfig) -> Result<()> {
        Ok(())
    }
    fn rename_window(&self, _: &str) -> Result<()> {
        Ok(())
    }
}
