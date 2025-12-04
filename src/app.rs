use anyhow::Result;
use clap::Parser;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::command::{CommandChecker, SystemCommandChecker};
use crate::environment::{Environment, SystemEnvironment};
use crate::ghq::SystemGhqClient;
use crate::selection::select_repository;
use crate::shell::{ShellExecutor, SystemShellExecutor};
use crate::tmux::{NoopTmuxClient, SystemTmuxClient, TmuxClient, WindowConfig};

#[derive(Parser)]
#[command(name = "gh-ghq-cd")]
#[command(about = "cd into ghq managed repositories")]
struct Args {
    /// Open in new tmux window (only works inside tmux)
    #[arg(short = 'n', long = "new-window")]
    new_window: bool,
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
            "{}: -nw is deprecated, use -n or --new-window instead",
            "warning".yellow().bold()
        );
    }

    let args = Args::parse_from(args);

    // Setup dependencies
    let env = SystemEnvironment;
    let checker = SystemCommandChecker;
    let ghq = SystemGhqClient;
    let shell = SystemShellExecutor;

    // Check if running inside tmux
    let use_tmux = env.var("TMUX").is_some();
    let tmux: Box<dyn TmuxClient> = if use_tmux {
        Box::new(SystemTmuxClient)
    } else {
        Box::new(NoopTmuxClient)
    };

    run_with_deps(&args, use_tmux, &env, &checker, &ghq, tmux.as_ref(), &shell)
}

fn run_with_deps(
    args: &Args,
    use_tmux: bool,
    env: &dyn Environment,
    checker: &dyn CommandChecker,
    ghq: &SystemGhqClient,
    tmux: &dyn TmuxClient,
    shell: &dyn ShellExecutor,
) -> Result<()> {
    // Check required commands
    checker.check("ghq")?;

    // Select repository using skim
    let selected = select_repository(ghq)?;

    if selected.is_empty() {
        return Ok(());
    }

    handle_selection(&selected, args.new_window, use_tmux, env, tmux, shell)
}

fn handle_selection(
    selected: &str,
    new_window_flag: bool,
    use_tmux: bool,
    env: &dyn Environment,
    tmux: &dyn TmuxClient,
    shell: &dyn ShellExecutor,
) -> Result<()> {
    let new_window = new_window_flag && use_tmux;

    let repo_name = Path::new(selected)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(selected);

    if new_window {
        let cfg = WindowConfig::new(repo_name, selected);
        tmux.new_window(&cfg)?;
    } else {
        // Change directory and start shell
        env.set_current_dir(selected)?;

        if use_tmux {
            tmux.rename_window(repo_name)?
        }

        let shell_path = env.var("SHELL").unwrap_or_else(|| String::from("/bin/sh"));
        shell.exec(&shell_path)?;
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

        fn with_var(mut self, key: &str, value: &str) -> Self {
            self.vars.insert(key.to_string(), value.to_string());
            self
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
        new_window_calls: RefCell<Vec<String>>,
        rename_window_calls: RefCell<Vec<String>>,
    }

    impl MockTmuxClient {
        fn new() -> Self {
            Self {
                new_window_calls: RefCell::new(Vec::new()),
                rename_window_calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl TmuxClient for MockTmuxClient {
        fn new_window(&self, cfg: &WindowConfig) -> Result<()> {
            self.new_window_calls.borrow_mut().push(cfg.name.clone());
            Ok(())
        }

        fn rename_window(&self, name: &str) -> Result<()> {
            self.rename_window_calls.borrow_mut().push(name.to_string());
            Ok(())
        }
    }

    struct MockShellExecutor {
        exec_calls: RefCell<Vec<String>>,
    }

    impl MockShellExecutor {
        fn new() -> Self {
            Self {
                exec_calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ShellExecutor for MockShellExecutor {
        fn exec(&self, shell: &str) -> Result<()> {
            self.exec_calls.borrow_mut().push(shell.to_string());
            Ok(())
        }
    }

    #[test]
    fn test_handle_selection_new_window_in_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();
        let shell = MockShellExecutor::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            true, // new_window_flag
            true, // use_tmux
            &env,
            &tmux,
            &shell,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_window_calls.borrow().len(), 1);
        assert_eq!(tmux.new_window_calls.borrow()[0], "repo");
        assert!(env.set_dir_calls.borrow().is_empty());
        assert!(shell.exec_calls.borrow().is_empty());
    }

    #[test]
    fn test_handle_selection_cd_in_tmux() {
        let env = MockEnvironment::new().with_var("SHELL", "/bin/zsh");
        let tmux = MockTmuxClient::new();
        let shell = MockShellExecutor::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            false, // new_window_flag
            true,  // use_tmux
            &env,
            &tmux,
            &shell,
        );

        assert!(result.is_ok());
        assert!(tmux.new_window_calls.borrow().is_empty());
        assert_eq!(tmux.rename_window_calls.borrow().len(), 1);
        assert_eq!(tmux.rename_window_calls.borrow()[0], "repo");
        assert_eq!(env.set_dir_calls.borrow().len(), 1);
        assert_eq!(
            env.set_dir_calls.borrow()[0],
            "/home/user/ghq/github.com/owner/repo"
        );
        assert_eq!(shell.exec_calls.borrow().len(), 1);
        assert_eq!(shell.exec_calls.borrow()[0], "/bin/zsh");
    }

    #[test]
    fn test_handle_selection_cd_outside_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();
        let shell = MockShellExecutor::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            false, // new_window_flag
            false, // use_tmux
            &env,
            &tmux,
            &shell,
        );

        assert!(result.is_ok());
        assert!(tmux.new_window_calls.borrow().is_empty());
        assert!(tmux.rename_window_calls.borrow().is_empty());
        assert_eq!(env.set_dir_calls.borrow().len(), 1);
        assert_eq!(shell.exec_calls.borrow().len(), 1);
        assert_eq!(shell.exec_calls.borrow()[0], "/bin/sh"); // default shell
    }

    #[test]
    fn test_handle_selection_new_window_flag_ignored_outside_tmux() {
        let env = MockEnvironment::new();
        let tmux = MockTmuxClient::new();
        let shell = MockShellExecutor::new();

        let result = handle_selection(
            "/home/user/ghq/github.com/owner/repo",
            true,  // new_window_flag - should be ignored
            false, // use_tmux
            &env,
            &tmux,
            &shell,
        );

        assert!(result.is_ok());
        // new_window should NOT be called because use_tmux is false
        assert!(tmux.new_window_calls.borrow().is_empty());
        assert_eq!(env.set_dir_calls.borrow().len(), 1);
        assert_eq!(shell.exec_calls.borrow().len(), 1);
    }
}
