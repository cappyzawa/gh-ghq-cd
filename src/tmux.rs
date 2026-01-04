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
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
    fn rename_window(&self, name: &str) -> Result<()>;
    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
}

pub struct SystemTmuxClient;
pub struct NoopTmuxClient;

impl TmuxClient for SystemTmuxClient {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        runner.run("tmux", &["new-window", "-n", &cfg.name, "-c", start_dir])?;

        // If pane_count >= 2, split the new window into 2 panes
        // (the new window itself is the "lane", so we only need to split it)
        if pane_count >= 2 {
            // Split direction:
            // - vertical (default): -v (split top/bottom)
            // - horizontal: -h (split left/right)
            let split = if horizontal { "-h" } else { "-v" };
            runner.run("tmux", &["split-window", split, "-c", start_dir])?;

            // Navigate and set titles for both panes
            let nav_to_first = if horizontal { "-L" } else { "-U" };
            let nav_to_second = if horizontal { "-R" } else { "-D" };

            runner.run("tmux", &["select-pane", nav_to_first])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            runner.run("tmux", &["select-pane", nav_to_second])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            // Return to first pane (focus)
            runner.run("tmux", &["select-pane", nav_to_first])?;

            // Equalize pane sizes
            runner.run("tmux", &["select-layout", "-E"])?;
        }

        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("tmux", &["rename-window", name])?;
        Ok(())
    }

    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        // Primary split direction:
        // - vertical (default): -hf (horizontal split with full height, creates left/right)
        // - horizontal: -vf (vertical split with full width, creates top/bottom)
        let primary_split = if horizontal { "-vf" } else { "-hf" };
        runner.run("tmux", &["split-window", primary_split, "-c", start_dir])?;

        // Set pane title for the new pane
        runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

        if pane_count >= 2 {
            // Secondary split (perpendicular to primary):
            // - vertical primary: -v (split top/bottom within the new pane)
            // - horizontal primary: -h (split left/right within the new pane)
            let secondary_split = if horizontal { "-h" } else { "-v" };
            runner.run("tmux", &["split-window", secondary_split, "-c", start_dir])?;

            // Navigate and set titles for both sub-panes
            let nav_to_first = if horizontal { "-L" } else { "-U" };
            let nav_to_second = if horizontal { "-R" } else { "-D" };

            runner.run("tmux", &["select-pane", nav_to_first])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            runner.run("tmux", &["select-pane", nav_to_second])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            // Return to first sub-pane (focus)
            runner.run("tmux", &["select-pane", nav_to_first])?;
        }

        // Equalize pane sizes
        runner.run("tmux", &["select-layout", "-E"])?;

        Ok(())
    }
}

impl TmuxClient for NoopTmuxClient {
    fn new_window(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
    fn rename_window(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn new_pane(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
}
