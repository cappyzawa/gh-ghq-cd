use anyhow::Result;
use clap::Parser;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::command::{CommandChecker, CommandRunner, SystemCommandChecker, SystemCommandRunner};
use crate::environment::{Environment, SystemEnvironment};
use crate::multiplexer::{Multiplexer, NoopClient, TmuxClient, WindowConfig};
use crate::selection::select_repository;
use crate::shell;

/// Mode of operation for tmux
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TmuxMode {
    /// Use current pane (cd + window rename)
    #[default]
    CurrentPane,
    /// Create new window with optional pane split
    NewWindow { count: u8, horizontal: bool },
    /// Create new pane with specified pane count and orientation
    NewPane { count: u8, horizontal: bool },
}

#[derive(Parser)]
#[command(name = "gh-ghq-cd")]
#[command(about = "cd into ghq managed repositories")]
struct Args {
    /// Open in new tmux window (only works inside tmux)
    #[arg(short = 'w', long = "new-window")]
    new_window: bool,

    /// [DEPRECATED] Use -w instead
    #[arg(short = 'n', hide = true)]
    deprecated_new_window: bool,

    /// Open in new tmux pane (1 = single pane, 2 = split into 2 panes)
    #[arg(short = 'p', long = "new-pane", num_args = 0..=1, default_missing_value = "1", value_parser = clap::value_parser!(u8).range(1..=2))]
    new_pane: Option<u8>,

    /// Use vertical split (default, only with -p)
    #[arg(
        short = 'V',
        long = "vertical",
        requires = "new_pane",
        conflicts_with = "horizontal"
    )]
    vertical: bool,

    /// Use horizontal split (only with -p)
    #[arg(
        short = 'H',
        long = "horizontal",
        requires = "new_pane",
        conflicts_with = "vertical"
    )]
    horizontal: bool,

    /// Command to run in the new pane/window
    #[arg(short = 'c', long = "command")]
    command: Option<String>,
}

impl Args {
    fn tmux_mode(&self) -> TmuxMode {
        let is_new_window = self.new_window || self.deprecated_new_window;

        if let Some(count) = self.new_pane {
            if is_new_window {
                // -w -p: new window with pane split
                TmuxMode::NewWindow {
                    count,
                    horizontal: self.horizontal,
                }
            } else {
                // -p only: pane split in current window
                TmuxMode::NewPane {
                    count,
                    horizontal: self.horizontal,
                }
            }
        } else if is_new_window {
            // -w only: new window without pane split
            TmuxMode::NewWindow {
                count: 0,
                horizontal: false,
            }
        } else {
            TmuxMode::CurrentPane
        }
    }
}

/// Entry point for the application
pub fn run() -> Result<()> {
    let mut has_deprecated_nw = false;
    let args: Vec<String> = std::env::args()
        .map(|arg| {
            if arg == "-nw" {
                has_deprecated_nw = true;
                "--new-window".to_string()
            } else {
                arg
            }
        })
        .collect();

    if has_deprecated_nw {
        eprintln!(
            "{}: -nw is deprecated, use -w or --new-window instead",
            "warning".yellow().bold()
        );
    }

    let args = Args::parse_from(args);

    // Show deprecation warning for -n
    if args.deprecated_new_window {
        eprintln!(
            "{}: -n is deprecated, use -w or --new-window instead",
            "warning".yellow().bold()
        );
    }

    // Validate: -c cannot be used with multiple panes (-p 2)
    if let Some(count) = args.new_pane
        && count >= 2
        && args.command.is_some()
    {
        anyhow::bail!("-c/--command cannot be used with multiple panes (-p 2)");
    }

    // Setup dependencies
    let env = SystemEnvironment;
    let checker = SystemCommandChecker;
    let runner = SystemCommandRunner;

    // Check if running inside tmux
    let use_tmux = env.var("TMUX").is_some();
    let tmux: Box<dyn Multiplexer> = if use_tmux {
        Box::new(TmuxClient)
    } else {
        Box::new(NoopClient)
    };

    let mode = args.tmux_mode();
    let command = args.command.as_deref();
    run_with_deps(
        mode,
        command,
        use_tmux,
        &env,
        &checker,
        &runner,
        tmux.as_ref(),
    )
}

fn run_with_deps(
    mode: TmuxMode,
    command: Option<&str>,
    use_tmux: bool,
    env: &dyn Environment,
    checker: &dyn CommandChecker,
    runner: &dyn CommandRunner,
    mux: &dyn Multiplexer,
) -> Result<()> {
    // Check required commands
    checker.check("ghq")?;
    checker.check("fzf")?;

    // Select repository using fzf
    let selected = select_repository(runner, checker)?;

    if selected.is_empty() {
        return Ok(());
    }

    handle_selection(&selected, mode, command, use_tmux, env, mux)
}

fn handle_selection(
    selected: &str,
    mode: TmuxMode,
    command: Option<&str>,
    use_tmux: bool,
    env: &dyn Environment,
    mux: &dyn Multiplexer,
) -> Result<()> {
    let repo_name = Path::new(selected)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(selected);

    // Apply tmux mode only when inside tmux
    let effective_mode = if use_tmux {
        mode
    } else {
        TmuxMode::CurrentPane
    };

    match effective_mode {
        TmuxMode::NewWindow { count, horizontal } => {
            let cfg = WindowConfig::new(repo_name, selected);
            mux.new_window(&cfg, count, horizontal)?;
            if let Some(cmd) = command {
                mux.send_keys(cmd)?;
            }
        }
        TmuxMode::NewPane { count, horizontal } => {
            let cfg = WindowConfig::new(repo_name, selected);
            mux.new_pane(&cfg, count, horizontal)?;
            if let Some(cmd) = command {
                mux.send_keys(cmd)?;
            }
        }
        TmuxMode::CurrentPane => {
            // Change directory and start shell
            env.set_current_dir(selected)?;

            if use_tmux {
                mux.rename_window(repo_name)?
            }

            let shell_path = env.var("SHELL").unwrap_or_else(|| String::from("/bin/sh"));
            shell::exec(&shell_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct MockEnvironment {
        vars: std::collections::HashMap<String, String>,
        set_dir_calls: RefCell<Vec<String>>,
    }

    impl MockEnvironment {
        fn new() -> Self {
            Self {
                vars: std::collections::HashMap::new(),
                set_dir_calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Environment for MockEnvironment {
        fn var(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }

        fn set_current_dir(&self, path: &str) -> Result<()> {
            self.set_dir_calls.borrow_mut().push(path.to_string());
            Ok(())
        }
    }

    struct MockTmuxClient {
        new_window_calls: RefCell<Vec<(String, u8, bool)>>,
        rename_window_calls: RefCell<Vec<String>>,
        new_pane_calls: RefCell<Vec<(String, u8, bool)>>,
        send_keys_calls: RefCell<Vec<String>>,
    }

    impl MockTmuxClient {
        fn new() -> Self {
            Self {
                new_window_calls: RefCell::new(Vec::new()),
                rename_window_calls: RefCell::new(Vec::new()),
                new_pane_calls: RefCell::new(Vec::new()),
                send_keys_calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Multiplexer for MockTmuxClient {
        fn new_window(&self, cfg: &WindowConfig, count: u8, horizontal: bool) -> Result<()> {
            self.new_window_calls
                .borrow_mut()
                .push((cfg.name.clone(), count, horizontal));
            Ok(())
        }

        fn rename_window(&self, name: &str) -> Result<()> {
            self.rename_window_calls.borrow_mut().push(name.to_string());
            Ok(())
        }

        fn new_pane(&self, cfg: &WindowConfig, count: u8, horizontal: bool) -> Result<()> {
            self.new_pane_calls
                .borrow_mut()
                .push((cfg.name.clone(), count, horizontal));
            Ok(())
        }

        fn send_keys(&self, keys: &str) -> Result<()> {
            self.send_keys_calls.borrow_mut().push(keys.to_string());
            Ok(())
        }
    }

    #[test]
    fn test_handle_selection_new_window_in_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            TmuxMode::NewWindow {
                count: 0,
                horizontal: false,
            },
            None,
            true,
            &env,
            &tmux,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_window_calls.borrow().len(), 1);
        assert_eq!(
            tmux.new_window_calls.borrow()[0],
            ("repo".to_string(), 0, false)
        );
        assert!(env.set_dir_calls.borrow().is_empty());
        assert!(tmux.new_pane_calls.borrow().is_empty());
        assert!(tmux.send_keys_calls.borrow().is_empty());
    }

    #[test]
    fn test_handle_selection_new_window_with_panes_in_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            TmuxMode::NewWindow {
                count: 2,
                horizontal: true,
            },
            None,
            true,
            &env,
            &tmux,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_window_calls.borrow().len(), 1);
        assert_eq!(
            tmux.new_window_calls.borrow()[0],
            ("repo".to_string(), 2, true)
        );
        assert!(env.set_dir_calls.borrow().is_empty());
        assert!(tmux.new_pane_calls.borrow().is_empty());
        assert!(tmux.send_keys_calls.borrow().is_empty());
    }

    #[test]
    fn test_handle_selection_new_pane_in_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            TmuxMode::NewPane {
                count: 2,
                horizontal: false,
            },
            None,
            true,
            &env,
            &tmux,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_pane_calls.borrow().len(), 1);
        assert_eq!(
            tmux.new_pane_calls.borrow()[0],
            ("repo".to_string(), 2, false)
        );
        assert!(env.set_dir_calls.borrow().is_empty());
        assert!(tmux.new_window_calls.borrow().is_empty());
        assert!(tmux.send_keys_calls.borrow().is_empty());
    }

    #[test]
    fn test_handle_selection_with_command() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            TmuxMode::NewWindow {
                count: 0,
                horizontal: false,
            },
            Some("claude"),
            true,
            &env,
            &tmux,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_window_calls.borrow().len(), 1);
        assert_eq!(tmux.send_keys_calls.borrow().len(), 1);
        assert_eq!(tmux.send_keys_calls.borrow()[0], "claude");
    }

    #[test]
    fn test_args_tmux_mode() {
        // -p 2
        let args = Args {
            new_window: false,
            deprecated_new_window: false,
            new_pane: Some(2),
            vertical: false,
            horizontal: false,
            command: None,
        };
        assert_eq!(
            args.tmux_mode(),
            TmuxMode::NewPane {
                count: 2,
                horizontal: false
            }
        );

        // -p -H
        let args = Args {
            new_window: false,
            deprecated_new_window: false,
            new_pane: Some(1),
            vertical: false,
            horizontal: true,
            command: None,
        };
        assert_eq!(
            args.tmux_mode(),
            TmuxMode::NewPane {
                count: 1,
                horizontal: true
            }
        );

        // -w
        let args = Args {
            new_window: true,
            deprecated_new_window: false,
            new_pane: None,
            vertical: false,
            horizontal: false,
            command: None,
        };
        assert_eq!(
            args.tmux_mode(),
            TmuxMode::NewWindow {
                count: 0,
                horizontal: false
            }
        );

        // -w -p
        let args = Args {
            new_window: true,
            deprecated_new_window: false,
            new_pane: Some(1),
            vertical: false,
            horizontal: false,
            command: None,
        };
        assert_eq!(
            args.tmux_mode(),
            TmuxMode::NewWindow {
                count: 1,
                horizontal: false
            }
        );

        // -w -p 2 -H
        let args = Args {
            new_window: true,
            deprecated_new_window: false,
            new_pane: Some(2),
            vertical: false,
            horizontal: true,
            command: None,
        };
        assert_eq!(
            args.tmux_mode(),
            TmuxMode::NewWindow {
                count: 2,
                horizontal: true
            }
        );

        // no flags
        let args = Args {
            new_window: false,
            deprecated_new_window: false,
            new_pane: None,
            vertical: false,
            horizontal: false,
            command: None,
        };
        assert_eq!(args.tmux_mode(), TmuxMode::CurrentPane);
    }
}
