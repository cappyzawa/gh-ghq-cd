use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::command::{CommandRunner, SystemCommandRunner};

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
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        runner.run("tmux", &["new-window", "-n", &cfg.name, "-c", start_dir])?;
        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("tmux", &["rename-window", name])?;
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
