use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::command::{CommandChecker, CommandRunner};
use crate::ghq;

/// Available preview viewers for README display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewViewer {
    Bat,
    Cat,
}

impl PreviewViewer {
    /// Detect the best available viewer
    /// Priority: bat > cat
    pub fn detect(checker: &dyn CommandChecker) -> Self {
        if checker.check("bat").is_ok() {
            Self::Bat
        } else {
            Self::Cat
        }
    }

    /// Generate the preview command for fzf
    /// The `{}` placeholder will be replaced with the path
    pub fn command(&self) -> &'static str {
        match self {
            Self::Bat => {
                "bat --style=plain --color=always {}/README.md 2>/dev/null || echo 'No README.md'"
            }
            Self::Cat => "cat {}/README.md 2>/dev/null || echo 'No README.md'",
        }
    }
}

/// Represents an item that can be displayed and selected
struct SelectableItem {
    display: String,
    value: String,
}

/// Run fzf with the given items and preview command
fn run_fzf(items: &[SelectableItem], preview_cmd: &str) -> Result<Option<String>> {
    let mut cmd = Command::new("fzf");

    // Use tab as delimiter, show only first field (display)
    cmd.arg("--delimiter=\t")
        .arg("--with-nth=1")
        .arg("--reverse");

    // {2} refers to the second tab-separated field (full_path)
    let preview_script = preview_cmd.replace("{}", "{2}");
    cmd.arg("--preview").arg(&preview_script);

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("failed to spawn fzf")?;

    // Write items to stdin: display\tvalue
    {
        let stdin = child.stdin.as_mut().context("failed to get stdin")?;
        for item in items {
            writeln!(stdin, "{}\t{}", item.display, item.value)?;
        }
    }

    let output = child.wait_with_output().context("failed to wait for fzf")?;

    if !output.status.success() {
        // fzf returns non-zero on abort (Ctrl-C, Esc)
        return Ok(None);
    }

    let selected = String::from_utf8_lossy(&output.stdout);
    let selected = selected.trim();

    if selected.is_empty() {
        return Ok(None);
    }

    // Extract value (second field) from selected line
    let value = selected
        .split('\t')
        .nth(1)
        .map(|s| s.to_string())
        .unwrap_or_else(|| selected.to_string());

    Ok(Some(value))
}

/// Select a repository interactively using fzf
pub fn select_repository(
    runner: &dyn CommandRunner,
    checker: &dyn CommandChecker,
) -> Result<String> {
    let roots = ghq::roots(runner)?;
    let repos = ghq::list_full_path(runner)?;

    let items: Vec<SelectableItem> = repos
        .iter()
        .map(|full_path| {
            let display_path = roots
                .iter()
                .find_map(|root| full_path.strip_prefix(root))
                .map(|stripped| stripped.trim_start_matches('/').to_string())
                .unwrap_or_else(|| full_path.to_string());

            SelectableItem {
                display: display_path,
                value: full_path.to_string(),
            }
        })
        .collect();

    let viewer = PreviewViewer::detect(checker);
    let selected = run_fzf(&items, viewer.command())?;

    Ok(selected.unwrap_or_default())
}
