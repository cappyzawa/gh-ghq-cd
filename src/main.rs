use anyhow::{Context, Result, bail};
use clap::Parser;
use owo_colors::OwoColorize;
use std::env;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

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
    check_command("fzf")?;

    // Select repository using fzf
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

fn select_repository() -> Result<String> {
    // Get preview command (bat or cat)
    let preview_cmd = if which::which("bat").is_ok() {
        "bat"
    } else {
        "cat"
    };

    // Run ghq list --full-path
    let ghq = Command::new("ghq")
        .args(["list", "--full-path"])
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to run ghq")?;

    // Pipe to fzf
    let fzf = Command::new("fzf")
        .args([
            "--reverse",
            "--preview",
            &format!(
                "{} {{}}/README.md 2>/dev/null || echo 'No README.md'",
                preview_cmd
            ),
        ])
        .stdin(ghq.stdout.unwrap())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to run fzf")?;

    let output = fzf.wait_with_output().context("failed to wait for fzf")?;

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
