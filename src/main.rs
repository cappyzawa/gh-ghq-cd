use anyhow::{Context, Result, bail};
use clap::Parser;
use owo_colors::OwoColorize;
use skim::prelude::*;
use std::borrow::Cow;
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use which::which;

use crate::tmux::NoopTmuxClient;
use crate::tmux::SystemTmuxClient;
use crate::tmux::TmuxClient;
use crate::tmux::WindowConfig;

mod tmux;

/// Custom SkimItem that displays short path but returns full path
struct RepoItem {
    full_path: String,
    display_path: String,
}

impl SkimItem for RepoItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display_path)
    }

    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.full_path)
    }
}

#[derive(Parser)]
#[command(name = "gh-ghq-cd")]
#[command(about = "cd into ghq managed repositories")]
struct Args {
    /// Open in new tmux window (only works inside tmux)
    #[arg(short = 'n', long = "new-window")]
    new_window: bool,
}

fn main() -> Result<()> {
    // Check for deprecated -nw flag and warn
    let raw_args: Vec<String> = env::args().collect();
    let has_deprecated_nw = raw_args.iter().any(|arg| arg == "-nw");

    let args: Vec<String> = raw_args
        .into_iter()
        .map(|arg| {
            if arg == "-nw" {
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

    // Check if running inside tmux
    let use_tmux = env::var("TMUX").is_ok();
    let new_window = args.new_window && use_tmux;

    let tmux: Box<dyn TmuxClient> = if use_tmux {
        Box::new(SystemTmuxClient)
    } else {
        Box::new(NoopTmuxClient)
    };

    // Check required commands
    check_command("ghq")?;

    // Select repository using skim
    let selected = select_repository()?;

    if selected.is_empty() {
        // User cancelled selection
        std::process::exit(1);
    }

    let repo_name = Path::new(&selected)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&selected);

    if new_window {
        let cfg = WindowConfig::new(repo_name, &selected);
        tmux.new_window(&cfg)?;
    } else {
        // Change directory and start shell
        env::set_current_dir(&selected).with_context(|| format!("failed to cd to {}", selected))?;

        if use_tmux {
            tmux.rename_window(repo_name)?
        }

        exec_shell()?;
    }

    Ok(())
}

fn check_command(cmd: &str) -> Result<()> {
    which(cmd).with_context(|| format!("{} not found on the system", cmd))?;
    Ok(())
}

fn preview_readme(items: Vec<Arc<dyn SkimItem>>) -> Vec<AnsiString<'static>> {
    let Some(item) = items.first() else {
        return vec!["No item selected".to_string().into()];
    };

    let repo_path = item.output().to_string();
    let readme_path = Path::new(&repo_path).join("README.md");

    match fs::read_to_string(&readme_path) {
        Ok(content) => {
            let rendered = termimad::term_text(&content);
            rendered
                .to_string()
                .lines()
                .map(AnsiString::parse)
                .collect()
        }
        Err(_) => vec!["No README.md".to_string().into()],
    }
}

fn select_repository() -> Result<String> {
    // Get ghq root paths (supports multiple roots)
    let root_output = Command::new("ghq")
        .args(["root", "--all"])
        .output()
        .context("failed to run ghq root")?;

    if !root_output.status.success() {
        bail!("ghq root failed");
    }

    let roots: Vec<String> = String::from_utf8_lossy(&root_output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    // Run ghq list --full-path and collect output
    let ghq_output = Command::new("ghq")
        .args(["list", "--full-path"])
        .output()
        .context("failed to run ghq")?;

    if !ghq_output.status.success() {
        bail!("ghq list failed");
    }

    // Build RepoItem list with display paths (path without ghq root prefix)
    let items: Vec<Arc<dyn SkimItem>> = String::from_utf8_lossy(&ghq_output.stdout)
        .lines()
        .map(|full_path| {
            // Find matching root and strip it from full path
            let display_path = roots
                .iter()
                .find_map(|root| full_path.strip_prefix(root))
                .map(|stripped| stripped.trim_start_matches('/').to_string())
                .unwrap_or_else(|| full_path.to_string());

            Arc::new(RepoItem {
                full_path: full_path.to_string(),
                display_path,
            }) as Arc<dyn SkimItem>
        })
        .collect();

    // Configure skim options with Rust-based preview function
    let options = SkimOptionsBuilder::default()
        .reverse(true)
        .preview_fn(Some(preview_readme.into()))
        .build()
        .context("failed to build skim options")?;

    // Create receiver for items
    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for item in items {
        let _ = tx.send(item);
    }
    drop(tx);

    // Run skim fuzzy finder
    let selected_items = Skim::run_with(&options, Some(rx))
        .filter(|out| !out.is_abort)
        .map(|out| out.selected_items)
        .unwrap_or_default();

    // Get the selected repository path (full path from output())
    let selected = selected_items
        .first()
        .map(|item| item.output().to_string())
        .unwrap_or_default();

    Ok(selected)
}

fn exec_shell() -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // exec replaces the current process
    let err = Command::new(&shell).exec();

    // If we get here, exec failed
    bail!("failed to exec {}: {}", shell, err);
}
