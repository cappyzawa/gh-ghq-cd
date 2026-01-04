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

pub trait Multiplexer {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
    fn rename_window(&self, name: &str) -> Result<()>;
    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
    fn send_keys(&self, keys: &str) -> Result<()>;
}

pub struct TmuxClient;
pub struct ZellijClient;
pub struct NoopClient;

impl Multiplexer for TmuxClient {
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

    fn send_keys(&self, keys: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("tmux", &["send-keys", keys, "Enter"])?;
        Ok(())
    }
}

impl Multiplexer for ZellijClient {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        runner.run(
            "zellij",
            &["action", "new-tab", "--name", &cfg.name, "--cwd", start_dir],
        )?;

        // Set pane name for the initial pane
        runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

        // If pane_count >= 2, split the new tab into 2 panes
        if pane_count >= 2 {
            // Split direction:
            // - vertical (default): down (split top/bottom)
            // - horizontal: right (split left/right)
            let direction = if horizontal { "right" } else { "down" };
            runner.run(
                "zellij",
                &[
                    "action",
                    "new-pane",
                    "--direction",
                    direction,
                    "--cwd",
                    start_dir,
                ],
            )?;

            // Set pane name for the new pane
            runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

            // Move focus back to first pane
            let focus_direction = if horizontal { "left" } else { "up" };
            runner.run("zellij", &["action", "move-focus", focus_direction])?;
        }

        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("zellij", &["action", "rename-tab", name])?;
        Ok(())
    }

    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        // Primary split direction:
        // - vertical (default): right (split left/right)
        // - horizontal: down (split top/bottom)
        let primary_direction = if horizontal { "down" } else { "right" };
        runner.run(
            "zellij",
            &[
                "action",
                "new-pane",
                "--direction",
                primary_direction,
                "--cwd",
                start_dir,
            ],
        )?;

        // Set pane name for the new pane
        runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

        if pane_count >= 2 {
            // Secondary split (perpendicular to primary):
            let secondary_direction = if horizontal { "right" } else { "down" };
            runner.run(
                "zellij",
                &[
                    "action",
                    "new-pane",
                    "--direction",
                    secondary_direction,
                    "--cwd",
                    start_dir,
                ],
            )?;

            // Set pane name for the second pane
            runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

            // Move focus back to first sub-pane
            let focus_direction = if horizontal { "left" } else { "up" };
            runner.run("zellij", &["action", "move-focus", focus_direction])?;
        }

        Ok(())
    }

    fn send_keys(&self, keys: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        // Write the command characters
        runner.run("zellij", &["action", "write-chars", keys])?;
        // Send Enter key (newline = 10 in ASCII)
        runner.run("zellij", &["action", "write", "10"])?;
        Ok(())
    }
}

impl Multiplexer for NoopClient {
    fn new_window(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
    fn rename_window(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn new_pane(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
    fn send_keys(&self, _: &str) -> Result<()> {
        Ok(())
    }
}
