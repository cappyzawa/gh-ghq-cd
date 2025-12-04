use anyhow::Result;

use crate::command::CommandRunner;

/// Get all ghq root directories
pub fn roots(runner: &dyn CommandRunner) -> Result<Vec<String>> {
    let output = runner.run("ghq", &["root", "--all"])?;
    Ok(output.lines().map(String::from).collect())
}

/// Get all repositories with full paths
pub fn list_full_path(runner: &dyn CommandRunner) -> Result<Vec<String>> {
    let output = runner.run("ghq", &["list", "--full-path"])?;
    Ok(output.lines().map(String::from).collect())
}
