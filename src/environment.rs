use anyhow::{Context, Result};

pub trait Environment {
    fn var(&self, key: &str) -> Option<String>;
    fn set_current_dir(&self, path: &str) -> Result<()>;
}

pub struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set_current_dir(&self, path: &str) -> Result<()> {
        std::env::set_current_dir(path).with_context(|| format!("failed to cd to {}", path))
    }
}
