use anyhow::{Context, Result};
use skim::prelude::*;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::ghq::GhqClient;

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

/// Select a repository interactively using skim fuzzy finder
pub fn select_repository(ghq: &dyn GhqClient) -> Result<String> {
    let roots = ghq.roots()?;
    let repos = ghq.list_full_path()?;

    let items: Vec<Arc<dyn SkimItem>> = repos
        .iter()
        .map(|full_path| {
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

    let options = SkimOptionsBuilder::default()
        .reverse(true)
        .preview_fn(Some(preview_readme.into()))
        .build()
        .context("failed to build skim options")?;

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for item in items {
        if tx.send(item).is_err() {
            break;
        }
    }
    drop(tx);

    let selected_items = Skim::run_with(&options, Some(rx))
        .filter(|out| !out.is_abort)
        .map(|out| out.selected_items)
        .unwrap_or_default();

    let selected = selected_items
        .first()
        .map(|item| item.output().to_string())
        .unwrap_or_default();

    Ok(selected)
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
