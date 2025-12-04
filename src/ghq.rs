use anyhow::Result;

use crate::command::{CommandRunner, SystemCommandRunner};

pub trait GhqClient {
    fn roots(&self) -> Result<Vec<String>>;
    fn list_full_path(&self) -> Result<Vec<String>>;
}

pub struct SystemGhqClient;

impl GhqClient for SystemGhqClient {
    fn roots(&self) -> Result<Vec<String>> {
        let runner = SystemCommandRunner;
        let output = runner.run("ghq", &["root", "--all"])?;
        Ok(output.lines().map(String::from).collect())
    }

    fn list_full_path(&self) -> Result<Vec<String>> {
        let runner = SystemCommandRunner;
        let output = runner.run("ghq", &["list", "--full-path"])?;
        Ok(output.lines().map(String::from).collect())
    }
}
