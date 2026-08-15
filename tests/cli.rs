//! Process-level e2e tests.
//!
//! Only cover argument-parsing paths that return before reaching external
//! commands (ghq / fzf / tmux / zellij / herdr), since none of those are
//! available in CI. See src/app.rs::run() for the validation order:
//! the `-c`/`-p 2` conflict is checked before `checker.check("ghq")`, and
//! `--help` is handled entirely by clap before `Args::parse_from` returns.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gh-ghq-cd"))
}

#[test]
fn test_command_with_multiple_panes_is_rejected() {
    let output = bin().args(["-c", "foo", "-p", "2"]).output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("-c/--command cannot be used with multiple panes (-p 2)"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn test_help_lists_main_options() {
    let output = bin().arg("--help").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--new-window"), "stdout: {stdout}");
    assert!(stdout.contains("--new-pane"), "stdout: {stdout}");
    assert!(stdout.contains("--command"), "stdout: {stdout}");
}
