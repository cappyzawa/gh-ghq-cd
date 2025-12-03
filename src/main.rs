use anyhow::{Context, Result, bail};
use clap::Parser;
use owo_colors::OwoColorize;
use skim::prelude::*;
use std::env;
use std::fs;
use std::io::Cursor;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

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
        // Open in new tmux window
        tmux_new_window(&selected, repo_name)?;
    } else {
        // Change directory and start shell
        env::set_current_dir(&selected).with_context(|| format!("failed to cd to {}", selected))?;

        if use_tmux {
            tmux_rename_window(repo_name)?;
        }

        exec_shell()?;
    }

    Ok(())
}

fn check_command(cmd: &str) -> Result<()> {
    which::which(cmd).with_context(|| format!("{} not found on the system", cmd))?;
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
            rendered.to_string().lines().map(AnsiString::parse).collect()
        }
        Err(_) => vec!["No README.md".to_string().into()],
    }
}

fn select_repository() -> Result<String> {
    // Run ghq list --full-path and collect output
    let ghq_output = Command::new("ghq")
        .args(["list", "--full-path"])
        .output()
        .context("failed to run ghq")?;

    if !ghq_output.status.success() {
        bail!("ghq list failed");
    }

    let repositories = String::from_utf8_lossy(&ghq_output.stdout);

    // Configure skim options with Rust-based preview function
    let options = SkimOptionsBuilder::default()
        .reverse(true)
        .preview_fn(Some(preview_readme.into()))
        .build()
        .context("failed to build skim options")?;

    // Create item reader and feed repository list
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(repositories.into_owned()));

    // Run skim fuzzy finder
    let selected_items = Skim::run_with(&options, Some(items))
        .filter(|out| !out.is_abort)
        .map(|out| out.selected_items)
        .unwrap_or_default();

    // Get the selected repository path
    let selected = selected_items
        .first()
        .map(|item| item.output().to_string())
        .unwrap_or_default();

    Ok(selected)
}

fn tmux_new_window(dir: &str, repo_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["new-window", "-n", repo_name, "-c", dir])
        .status()
        .context("failed to run tmux new-window")?;

    if !status.success() {
        bail!("tmux new-window failed");
    }
    Ok(())
}

fn tmux_rename_window(repo_name: &str) -> Result<()> {
    let _ = Command::new("tmux")
        .args(["rename-window", repo_name])
        .status();
    // Ignore errors for rename-window
    Ok(())
}

fn exec_shell() -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // exec replaces the current process
    let err = Command::new(&shell).exec();

    // If we get here, exec failed
    bail!("failed to exec {}: {}", shell, err);
}
