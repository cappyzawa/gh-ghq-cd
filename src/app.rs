use anyhow::Result;
use clap::Parser;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::command::{CommandChecker, CommandRunner, SystemCommandChecker, SystemCommandRunner};
use crate::environment::{Environment, SystemEnvironment};
use multiplexer::{
    HerdrClient, Multiplexer, NoopClient, Orientation, PaneCount, TmuxClient, WindowConfig,
    ZellijClient,
};
use selection::select_repository;

mod multiplexer;
mod selection;
mod shell;

/// What the tool should do with the repository it selected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MultiplexerMode {
    /// Use current pane (cd + window rename)
    #[default]
    CurrentPane,
    /// Create a new window, split in two when an orientation is given
    NewWindow { split: Option<Orientation> },
    /// Create a new pane, split perpendicularly again when two are asked for
    NewPane {
        panes: PaneCount,
        orientation: Orientation,
    },
}

#[derive(Parser)]
#[command(name = "gh-ghq-cd")]
#[command(about = "cd into ghq managed repositories")]
struct Args {
    /// Open in new window (tmux) / tab (zellij) / workspace (herdr)
    #[arg(short = 'w', long = "new-window")]
    new_window: bool,

    /// [DEPRECATED] Use -w instead
    #[arg(short = 'n', hide = true)]
    deprecated_new_window: bool,

    /// Open in new pane (1 = single pane, 2 = split into 2 panes)
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
    fn orientation(&self) -> Orientation {
        if self.horizontal {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        }
    }

    fn mode(&self) -> MultiplexerMode {
        let is_new_window = self.new_window || self.deprecated_new_window;

        match (self.new_pane, is_new_window) {
            // A new window holds one pane already, so only -p 2 splits it
            (count, true) => MultiplexerMode::NewWindow {
                split: (count == Some(2)).then(|| self.orientation()),
            },
            (Some(2), false) => MultiplexerMode::NewPane {
                panes: PaneCount::Two,
                orientation: self.orientation(),
            },
            (Some(_), false) => MultiplexerMode::NewPane {
                panes: PaneCount::One,
                orientation: self.orientation(),
            },
            (None, false) => MultiplexerMode::CurrentPane,
        }
    }
}

/// Terminal multiplexer the tool is running inside
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultiplexerKind {
    Herdr,
    Zellij,
    Tmux,
    None,
}

fn detect_multiplexer(env: &dyn Environment) -> MultiplexerKind {
    if env.var("HERDR_ENV").is_some() {
        MultiplexerKind::Herdr
    } else if env.var("ZELLIJ").is_some() {
        MultiplexerKind::Zellij
    } else if env.var("TMUX").is_some() {
        MultiplexerKind::Tmux
    } else {
        MultiplexerKind::None
    }
}

/// Entry point for the application
pub(super) fn run() -> Result<()> {
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

    let multiplexer = detect_multiplexer(&env);
    let use_multiplexer = multiplexer != MultiplexerKind::None;

    let mux: Box<dyn Multiplexer> = match multiplexer {
        MultiplexerKind::Herdr => Box::new(HerdrClient::from_env(&env)),
        MultiplexerKind::Zellij => Box::new(ZellijClient::new()),
        MultiplexerKind::Tmux => Box::new(TmuxClient::new()),
        MultiplexerKind::None => Box::new(NoopClient),
    };

    let mode = args.mode();
    let command = args.command.as_deref();
    run_with_deps(
        mode,
        command,
        use_multiplexer,
        &env,
        &checker,
        &runner,
        mux.as_ref(),
    )
}

fn run_with_deps(
    mode: MultiplexerMode,
    command: Option<&str>,
    use_mux: bool,
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

    handle_selection(&selected, mode, command, use_mux, env, mux)
}

fn handle_selection(
    selected: &str,
    mode: MultiplexerMode,
    command: Option<&str>,
    use_mux: bool,
    env: &dyn Environment,
    mux: &dyn Multiplexer,
) -> Result<()> {
    let repo_name = Path::new(selected)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(selected);

    // Apply mode only when inside a terminal multiplexer
    let effective_mode = if use_mux {
        mode
    } else {
        MultiplexerMode::CurrentPane
    };

    match effective_mode {
        MultiplexerMode::NewWindow { split } => {
            let cfg = WindowConfig::new(repo_name, selected);
            mux.new_window(&cfg, split)?;
            if let Some(cmd) = command {
                mux.send_keys(cmd)?;
            }
        }
        MultiplexerMode::NewPane { panes, orientation } => {
            let cfg = WindowConfig::new(repo_name, selected);
            mux.new_pane(&cfg, panes, orientation)?;
            if let Some(cmd) = command {
                mux.send_keys(cmd)?;
            }
        }
        MultiplexerMode::CurrentPane => {
            // Change directory and start shell
            env.set_current_dir(selected)?;

            if use_mux {
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
        new_window_calls: RefCell<Vec<(String, Option<Orientation>)>>,
        rename_window_calls: RefCell<Vec<String>>,
        new_pane_calls: RefCell<Vec<(String, PaneCount, Orientation)>>,
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
        fn new_window(&self, cfg: &WindowConfig, split: Option<Orientation>) -> Result<()> {
            self.new_window_calls
                .borrow_mut()
                .push((cfg.name.clone(), split));
            Ok(())
        }

        fn rename_window(&self, name: &str) -> Result<()> {
            self.rename_window_calls.borrow_mut().push(name.to_string());
            Ok(())
        }

        fn new_pane(
            &self,
            cfg: &WindowConfig,
            panes: PaneCount,
            orientation: Orientation,
        ) -> Result<()> {
            self.new_pane_calls
                .borrow_mut()
                .push((cfg.name.clone(), panes, orientation));
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
            MultiplexerMode::NewWindow { split: None },
            None,
            true,
            &env,
            &tmux,
        );

        assert!(result.is_ok());
        assert_eq!(tmux.new_window_calls.borrow().len(), 1);
        assert_eq!(
            tmux.new_window_calls.borrow()[0],
            ("repo".to_string(), None)
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
            MultiplexerMode::NewWindow {
                split: Some(Orientation::Horizontal),
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
            ("repo".to_string(), Some(Orientation::Horizontal))
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
            MultiplexerMode::NewPane {
                panes: PaneCount::Two,
                orientation: Orientation::Vertical,
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
            ("repo".to_string(), PaneCount::Two, Orientation::Vertical)
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
            MultiplexerMode::NewWindow { split: None },
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
    fn test_detect_multiplexer_prefers_herdr() {
        let mut env = MockEnvironment::new();
        env.vars.insert("HERDR_ENV".to_string(), "1".to_string());
        env.vars.insert("ZELLIJ".to_string(), "0".to_string());
        env.vars
            .insert("TMUX".to_string(), "/tmp/tmux-501,1,0".to_string());

        assert_eq!(detect_multiplexer(&env), MultiplexerKind::Herdr);
    }

    #[test]
    fn test_detect_multiplexer_prefers_zellij_over_tmux() {
        let mut env = MockEnvironment::new();
        env.vars.insert("ZELLIJ".to_string(), "0".to_string());
        env.vars
            .insert("TMUX".to_string(), "/tmp/tmux-501,1,0".to_string());

        assert_eq!(detect_multiplexer(&env), MultiplexerKind::Zellij);
    }

    #[test]
    fn test_detect_multiplexer_without_a_multiplexer() {
        let env = MockEnvironment::new();

        assert_eq!(detect_multiplexer(&env), MultiplexerKind::None);
    }

    /// A new window opens with one pane already, so `-w -p` asks for nothing
    /// beyond `-w`; only `-p 2` splits it.
    #[test]
    fn test_new_window_with_a_single_pane_request_does_not_split() {
        let args = Args {
            new_window: true,
            deprecated_new_window: false,
            new_pane: Some(1),
            vertical: false,
            horizontal: true,
            command: None,
        };

        assert_eq!(args.mode(), MultiplexerMode::NewWindow { split: None });
    }

    #[test]
    fn test_args_mode() {
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
            args.mode(),
            MultiplexerMode::NewPane {
                panes: PaneCount::Two,
                orientation: Orientation::Vertical,
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
            args.mode(),
            MultiplexerMode::NewPane {
                panes: PaneCount::One,
                orientation: Orientation::Horizontal,
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
        assert_eq!(args.mode(), MultiplexerMode::NewWindow { split: None });

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
            args.mode(),
            MultiplexerMode::NewWindow {
                split: Some(Orientation::Horizontal)
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
        assert_eq!(args.mode(), MultiplexerMode::CurrentPane);
    }
}
