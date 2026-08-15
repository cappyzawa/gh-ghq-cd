use std::cell::RefCell;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::command::{CommandRunner, SystemCommandRunner};
use crate::environment::Environment;

pub struct WindowConfig {
    pub name: String,
    pub start_dir: PathBuf,
}

impl WindowConfig {
    pub fn new<S: Into<String>, P: Into<PathBuf>>(name: S, start_dir: P) -> Self {
        Self {
            name: name.into(),
            start_dir: start_dir.into(),
        }
    }
}

pub trait Multiplexer {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
    fn rename_window(&self, name: &str) -> Result<()>;
    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()>;
    fn send_keys(&self, keys: &str) -> Result<()>;
}

pub struct TmuxClient;
pub struct ZellijClient;
pub struct NoopClient;

/// Drives herdr (https://herdr.dev) through its CLI.
///
/// Unlike tmux and zellij, herdr commands never act on an implicit "current"
/// target: every pane operation takes a pane ID. IDs of created panes only
/// exist in the JSON response of the command that created them, so they are
/// read out and carried here.
pub struct HerdrClient<R: CommandRunner> {
    runner: R,
    workspace_id: Option<String>,
    pane_id: Option<String>,
    target_pane: RefCell<Option<String>>,
}

impl HerdrClient<SystemCommandRunner> {
    pub fn from_env(env: &dyn Environment) -> Self {
        Self::with_runner(
            SystemCommandRunner,
            env.var("HERDR_WORKSPACE_ID"),
            env.var("HERDR_PANE_ID"),
        )
    }
}

impl<R: CommandRunner> HerdrClient<R> {
    fn with_runner(runner: R, workspace_id: Option<String>, pane_id: Option<String>) -> Self {
        Self {
            runner,
            workspace_id,
            pane_id,
            target_pane: RefCell::new(None),
        }
    }

    fn split_pane(
        &self,
        target: &str,
        direction: SplitDirection,
        start_dir: &str,
    ) -> Result<String> {
        // The pane ID must precede the flags; herdr rejects a trailing
        // positional argument as an unknown option.
        let output = self.runner.run(
            "herdr",
            &[
                "pane",
                "split",
                target,
                "--direction",
                direction.arg(),
                "--cwd",
                start_dir,
            ],
        )?;
        id_of(&json_of(&output)?, "pane", "pane_id")
    }

    fn rename_pane(&self, pane: &str, name: &str) -> Result<()> {
        self.runner.run("herdr", &["pane", "rename", pane, name])?;
        Ok(())
    }

    /// herdr addresses focus by direction rather than by pane ID, so a pane is
    /// focused through one of its neighbors.
    fn focus_neighbor(&self, pane: &str, direction: &str) -> Result<()> {
        self.runner.run(
            "herdr",
            &["pane", "focus", "--direction", direction, "--pane", pane],
        )?;
        Ok(())
    }
}

/// Direction herdr splits a pane in, together with the direction that walks
/// back from the new pane to the one it was split off.
#[derive(Clone, Copy)]
enum SplitDirection {
    Down,
    Right,
}

impl SplitDirection {
    fn perpendicular(self) -> Self {
        match self {
            Self::Down => Self::Right,
            Self::Right => Self::Down,
        }
    }

    fn arg(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Right => "right",
        }
    }

    fn back_arg(self) -> &'static str {
        match self {
            Self::Down => "up",
            Self::Right => "left",
        }
    }
}

fn json_of(stdout: &str) -> Result<Value> {
    serde_json::from_str(stdout).context("herdr returned invalid JSON")
}

fn id_of(response: &Value, object: &str, field: &str) -> Result<String> {
    response["result"][object][field]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("herdr response carries no .result.{}.{}", object, field))
}

impl<R: CommandRunner> Multiplexer for HerdrClient<R> {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        let output = self.runner.run(
            "herdr",
            &[
                "workspace",
                "create",
                "--cwd",
                start_dir,
                "--label",
                &cfg.name,
            ],
        )?;
        let response = json_of(&output)?;
        let workspace = id_of(&response, "workspace", "workspace_id")?;
        let root_pane = id_of(&response, "root_pane", "pane_id")?;
        self.rename_pane(&root_pane, &cfg.name)?;
        *self.target_pane.borrow_mut() = Some(root_pane.clone());

        // If pane_count >= 2, split the new workspace into 2 panes
        if pane_count >= 2 {
            // Split direction:
            // - vertical (default): down (split top/bottom)
            // - horizontal: right (split left/right)
            let direction = if horizontal {
                SplitDirection::Right
            } else {
                SplitDirection::Down
            };
            let split = self.split_pane(&root_pane, direction, start_dir)?;
            self.rename_pane(&split, &cfg.name)?;

            // Return to the first pane
            self.focus_neighbor(&split, direction.back_arg())?;
        }

        self.runner
            .run("herdr", &["workspace", "focus", &workspace])?;

        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let workspace = self
            .workspace_id
            .as_deref()
            .context("HERDR_WORKSPACE_ID is not set")?;
        self.runner
            .run("herdr", &["workspace", "rename", workspace, name])?;
        Ok(())
    }

    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;
        let caller_pane = self
            .pane_id
            .as_deref()
            .context("HERDR_PANE_ID is not set")?;

        // Primary split direction:
        // - vertical (default): right (split left/right)
        // - horizontal: down (split top/bottom)
        let primary_direction = if horizontal {
            SplitDirection::Down
        } else {
            SplitDirection::Right
        };
        let pane = self.split_pane(caller_pane, primary_direction, start_dir)?;
        self.rename_pane(&pane, &cfg.name)?;
        *self.target_pane.borrow_mut() = Some(pane.clone());

        if pane_count >= 2 {
            let secondary_direction = primary_direction.perpendicular();
            let sub_pane = self.split_pane(&pane, secondary_direction, start_dir)?;
            self.rename_pane(&sub_pane, &cfg.name)?;

            // Return to the first sub-pane
            self.focus_neighbor(&sub_pane, secondary_direction.back_arg())?;
        } else {
            self.focus_neighbor(caller_pane, primary_direction.arg())?;
        }

        Ok(())
    }

    fn send_keys(&self, keys: &str) -> Result<()> {
        let target_pane = self.target_pane.borrow();
        let pane = target_pane
            .as_deref()
            .context("no herdr pane has been created to run the command in")?;
        self.runner.run("herdr", &["pane", "run", pane, keys])?;
        Ok(())
    }
}

impl Multiplexer for TmuxClient {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        runner.run("tmux", &["new-window", "-n", &cfg.name, "-c", start_dir])?;

        // If pane_count >= 2, split the new window into 2 panes
        // (the new window itself is the "lane", so we only need to split it)
        if pane_count >= 2 {
            // Split direction:
            // - vertical (default): -v (split top/bottom)
            // - horizontal: -h (split left/right)
            let split = if horizontal { "-h" } else { "-v" };
            runner.run("tmux", &["split-window", split, "-c", start_dir])?;

            // Navigate and set titles for both panes
            let nav_to_first = if horizontal { "-L" } else { "-U" };
            let nav_to_second = if horizontal { "-R" } else { "-D" };

            runner.run("tmux", &["select-pane", nav_to_first])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            runner.run("tmux", &["select-pane", nav_to_second])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            // Return to first pane (focus)
            runner.run("tmux", &["select-pane", nav_to_first])?;

            // Equalize pane sizes
            runner.run("tmux", &["select-layout", "-E"])?;
        }

        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("tmux", &["rename-window", name])?;
        Ok(())
    }

    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        // Primary split direction:
        // - vertical (default): -hf (horizontal split with full height, creates left/right)
        // - horizontal: -vf (vertical split with full width, creates top/bottom)
        let primary_split = if horizontal { "-vf" } else { "-hf" };
        runner.run("tmux", &["split-window", primary_split, "-c", start_dir])?;

        // Set pane title for the new pane
        runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

        if pane_count >= 2 {
            // Secondary split (perpendicular to primary):
            // - vertical primary: -v (split top/bottom within the new pane)
            // - horizontal primary: -h (split left/right within the new pane)
            let secondary_split = if horizontal { "-h" } else { "-v" };
            runner.run("tmux", &["split-window", secondary_split, "-c", start_dir])?;

            // Navigate and set titles for both sub-panes
            let nav_to_first = if horizontal { "-L" } else { "-U" };
            let nav_to_second = if horizontal { "-R" } else { "-D" };

            runner.run("tmux", &["select-pane", nav_to_first])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            runner.run("tmux", &["select-pane", nav_to_second])?;
            runner.run("tmux", &["select-pane", "-T", &cfg.name])?;

            // Return to first sub-pane (focus)
            runner.run("tmux", &["select-pane", nav_to_first])?;
        }

        // Equalize pane sizes
        runner.run("tmux", &["select-layout", "-E"])?;

        Ok(())
    }

    fn send_keys(&self, keys: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("tmux", &["send-keys", keys, "Enter"])?;
        Ok(())
    }
}

impl Multiplexer for ZellijClient {
    fn new_window(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        runner.run(
            "zellij",
            &["action", "new-tab", "--name", &cfg.name, "--cwd", start_dir],
        )?;

        // Set pane name for the initial pane
        runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

        // If pane_count >= 2, split the new tab into 2 panes
        if pane_count >= 2 {
            // Split direction:
            // - vertical (default): down (split top/bottom)
            // - horizontal: right (split left/right)
            let direction = if horizontal { "right" } else { "down" };
            runner.run(
                "zellij",
                &[
                    "action",
                    "new-pane",
                    "--direction",
                    direction,
                    "--cwd",
                    start_dir,
                ],
            )?;

            // Set pane name for the new pane
            runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

            // Move focus back to first pane
            let focus_direction = if horizontal { "left" } else { "up" };
            runner.run("zellij", &["action", "move-focus", focus_direction])?;
        }

        Ok(())
    }

    fn rename_window(&self, name: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        runner.run("zellij", &["action", "rename-tab", name])?;
        Ok(())
    }

    fn new_pane(&self, cfg: &WindowConfig, pane_count: u8, horizontal: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let start_dir = cfg
            .start_dir
            .to_str()
            .context("repository path contains invalid UTF-8")?;

        // Primary split direction:
        // - vertical (default): right (split left/right)
        // - horizontal: down (split top/bottom)
        let primary_direction = if horizontal { "down" } else { "right" };
        runner.run(
            "zellij",
            &[
                "action",
                "new-pane",
                "--direction",
                primary_direction,
                "--cwd",
                start_dir,
            ],
        )?;

        // Set pane name for the new pane
        runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

        if pane_count >= 2 {
            // Secondary split (perpendicular to primary):
            let secondary_direction = if horizontal { "right" } else { "down" };
            runner.run(
                "zellij",
                &[
                    "action",
                    "new-pane",
                    "--direction",
                    secondary_direction,
                    "--cwd",
                    start_dir,
                ],
            )?;

            // Set pane name for the second pane
            runner.run("zellij", &["action", "rename-pane", &cfg.name])?;

            // Move focus back to first sub-pane
            let focus_direction = if horizontal { "left" } else { "up" };
            runner.run("zellij", &["action", "move-focus", focus_direction])?;
        }

        Ok(())
    }

    fn send_keys(&self, keys: &str) -> Result<()> {
        let runner = SystemCommandRunner;
        // Write the command characters
        runner.run("zellij", &["action", "write-chars", keys])?;
        // Send Enter key (newline = 10 in ASCII)
        runner.run("zellij", &["action", "write", "10"])?;
        Ok(())
    }
}

impl Multiplexer for NoopClient {
    fn new_window(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
    fn rename_window(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn new_pane(&self, _: &WindowConfig, _: u8, _: bool) -> Result<()> {
        Ok(())
    }
    fn send_keys(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod herdr_tests {
    use super::*;
    use std::cell::RefCell;

    const CREATE_RESPONSE: &str = r#"{"id":"cli:workspace:create","result":{"root_pane":{"pane_id":"w3:p1","tab_id":"w3:t1","workspace_id":"w3"},"tab":{"tab_id":"w3:t1"},"type":"workspace_created","workspace":{"workspace_id":"w3","label":"repo"}}}"#;
    const SPLIT_P2_RESPONSE: &str = r#"{"id":"cli:pane:split","result":{"pane":{"pane_id":"w3:p2","tab_id":"w3:t1","workspace_id":"w3"},"type":"pane_info"}}"#;
    const SPLIT_P3_RESPONSE: &str = r#"{"id":"cli:pane:split","result":{"pane":{"pane_id":"w3:p3","tab_id":"w3:t1","workspace_id":"w3"},"type":"pane_info"}}"#;
    const RENAME_RESPONSE: &str =
        r#"{"id":"cli:pane:rename","result":{"pane":{"pane_id":"w3:p1"},"type":"pane_info"}}"#;

    struct MockCommandRunner {
        responses: RefCell<Vec<String>>,
        calls: RefCell<Vec<(String, Vec<String>)>>,
    }

    impl MockCommandRunner {
        fn new(responses: &[&str]) -> Self {
            Self {
                responses: RefCell::new(responses.iter().map(|s| s.to_string()).collect()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn assert_calls(&self, expected: &[(&str, Vec<&str>)]) {
            let actual = self.calls.borrow();
            let expected: Vec<(String, Vec<String>)> = expected
                .iter()
                .map(|(cmd, args)| {
                    (
                        cmd.to_string(),
                        args.iter().map(|a| a.to_string()).collect(),
                    )
                })
                .collect();
            assert_eq!(*actual, expected);
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run(&self, cmd: &str, args: &[&str]) -> Result<String> {
            self.calls.borrow_mut().push((
                cmd.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
            ));
            let mut responses = self.responses.borrow_mut();
            if responses.is_empty() {
                Ok(String::new())
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    fn cfg() -> WindowConfig {
        WindowConfig::new("repo", "/repos/repo")
    }

    fn client(runner: MockCommandRunner) -> HerdrClient<MockCommandRunner> {
        HerdrClient::with_runner(runner, Some("w0".to_string()), Some("w0:p1".to_string()))
    }
    const CREATE_CALL: [&str; 6] = [
        "workspace",
        "create",
        "--cwd",
        "/repos/repo",
        "--label",
        "repo",
    ];

    fn split_call<'a>(target: &'a str, direction: &'a str) -> Vec<&'a str> {
        vec![
            "pane",
            "split",
            target,
            "--direction",
            direction,
            "--cwd",
            "/repos/repo",
        ]
    }

    fn focus_call<'a>(pane: &'a str, direction: &'a str) -> Vec<&'a str> {
        vec!["pane", "focus", "--direction", direction, "--pane", pane]
    }

    #[test]
    fn test_new_window_creates_workspace_and_labels_root_pane() {
        let herdr = client(MockCommandRunner::new(&[CREATE_RESPONSE, RENAME_RESPONSE]));

        herdr.new_window(&cfg(), 0, false).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", CREATE_CALL.to_vec()),
            ("herdr", vec!["pane", "rename", "w3:p1", "repo"]),
            ("herdr", vec!["workspace", "focus", "w3"]),
        ]);
    }

    #[test]
    fn test_new_window_with_two_panes_splits_root_pane_downward() {
        let herdr = client(MockCommandRunner::new(&[
            CREATE_RESPONSE,
            RENAME_RESPONSE,
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_window(&cfg(), 2, false).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", CREATE_CALL.to_vec()),
            ("herdr", vec!["pane", "rename", "w3:p1", "repo"]),
            ("herdr", split_call("w3:p1", "down")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", focus_call("w3:p2", "up")),
            ("herdr", vec!["workspace", "focus", "w3"]),
        ]);
    }

    #[test]
    fn test_new_window_with_two_panes_horizontal_splits_rightward() {
        let herdr = client(MockCommandRunner::new(&[
            CREATE_RESPONSE,
            RENAME_RESPONSE,
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_window(&cfg(), 2, true).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", CREATE_CALL.to_vec()),
            ("herdr", vec!["pane", "rename", "w3:p1", "repo"]),
            ("herdr", split_call("w3:p1", "right")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", focus_call("w3:p2", "left")),
            ("herdr", vec!["workspace", "focus", "w3"]),
        ]);
    }

    #[test]
    fn test_new_pane_splits_calling_pane_rightward_and_focuses_it() {
        let herdr = client(MockCommandRunner::new(&[
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_pane(&cfg(), 1, false).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", split_call("w0:p1", "right")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", focus_call("w0:p1", "right")),
        ]);
    }

    #[test]
    fn test_new_pane_with_two_panes_splits_the_new_pane_perpendicularly() {
        let herdr = client(MockCommandRunner::new(&[
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
            SPLIT_P3_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_pane(&cfg(), 2, false).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", split_call("w0:p1", "right")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", split_call("w3:p2", "down")),
            ("herdr", vec!["pane", "rename", "w3:p3", "repo"]),
            ("herdr", focus_call("w3:p3", "up")),
        ]);
    }

    #[test]
    fn test_new_pane_horizontal_splits_down_then_right() {
        let herdr = client(MockCommandRunner::new(&[
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
            SPLIT_P3_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_pane(&cfg(), 2, true).unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", split_call("w0:p1", "down")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", split_call("w3:p2", "right")),
            ("herdr", vec!["pane", "rename", "w3:p3", "repo"]),
            ("herdr", focus_call("w3:p3", "left")),
        ]);
    }

    #[test]
    fn test_rename_window_renames_the_calling_workspace() {
        let herdr = client(MockCommandRunner::new(&[]));

        herdr.rename_window("repo").unwrap();

        herdr
            .runner
            .assert_calls(&[("herdr", vec!["workspace", "rename", "w0", "repo"])]);
    }

    #[test]
    fn test_send_keys_runs_the_command_in_the_primary_pane() {
        let herdr = client(MockCommandRunner::new(&[CREATE_RESPONSE, RENAME_RESPONSE]));

        herdr.new_window(&cfg(), 0, false).unwrap();
        herdr.send_keys("claude").unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", CREATE_CALL.to_vec()),
            ("herdr", vec!["pane", "rename", "w3:p1", "repo"]),
            ("herdr", vec!["workspace", "focus", "w3"]),
            ("herdr", vec!["pane", "run", "w3:p1", "claude"]),
        ]);
    }

    #[test]
    fn test_send_keys_targets_the_primary_pane_of_a_split_window() {
        let herdr = client(MockCommandRunner::new(&[
            CREATE_RESPONSE,
            RENAME_RESPONSE,
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_window(&cfg(), 2, false).unwrap();
        herdr.send_keys("claude").unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", CREATE_CALL.to_vec()),
            ("herdr", vec!["pane", "rename", "w3:p1", "repo"]),
            ("herdr", split_call("w3:p1", "down")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", focus_call("w3:p2", "up")),
            ("herdr", vec!["workspace", "focus", "w3"]),
            ("herdr", vec!["pane", "run", "w3:p1", "claude"]),
        ]);
    }

    #[test]
    fn test_send_keys_targets_the_primary_pane_created_by_new_pane() {
        let herdr = client(MockCommandRunner::new(&[
            SPLIT_P2_RESPONSE,
            RENAME_RESPONSE,
        ]));

        herdr.new_pane(&cfg(), 1, false).unwrap();
        herdr.send_keys("npm run dev").unwrap();

        herdr.runner.assert_calls(&[
            ("herdr", split_call("w0:p1", "right")),
            ("herdr", vec!["pane", "rename", "w3:p2", "repo"]),
            ("herdr", focus_call("w0:p1", "right")),
            ("herdr", vec!["pane", "run", "w3:p2", "npm run dev"]),
        ]);
    }

    #[test]
    fn test_send_keys_without_a_created_pane_fails() {
        let herdr = client(MockCommandRunner::new(&[]));

        assert!(herdr.send_keys("claude").is_err());
        herdr.runner.assert_calls(&[]);
    }

    #[test]
    fn test_commands_fail_when_herdr_context_is_missing() {
        let herdr = HerdrClient::with_runner(MockCommandRunner::new(&[]), None, None);

        assert!(herdr.rename_window("repo").is_err());
        assert!(herdr.new_pane(&cfg(), 1, false).is_err());
        herdr.runner.assert_calls(&[]);
    }

    #[test]
    fn test_new_window_fails_when_the_response_carries_no_pane_id() {
        let herdr = client(MockCommandRunner::new(&[
            r#"{"id":"cli:workspace:create","result":{"type":"workspace_created","workspace":{"workspace_id":"w3"}}}"#,
        ]));

        let err = herdr.new_window(&cfg(), 0, false).unwrap_err();

        assert_eq!(
            err.to_string(),
            "herdr response carries no .result.root_pane.pane_id"
        );
        herdr
            .runner
            .assert_calls(&[("herdr", CREATE_CALL.to_vec())]);
    }

    #[test]
    fn test_new_window_fails_when_the_response_carries_no_workspace_id() {
        let herdr = client(MockCommandRunner::new(&[
            r#"{"id":"cli:workspace:create","result":{"root_pane":{"pane_id":"w3:p1"},"type":"workspace_created"}}"#,
        ]));

        let err = herdr.new_window(&cfg(), 0, false).unwrap_err();

        assert_eq!(
            err.to_string(),
            "herdr response carries no .result.workspace.workspace_id"
        );
        herdr
            .runner
            .assert_calls(&[("herdr", CREATE_CALL.to_vec())]);
    }

    #[test]
    fn test_new_window_stops_on_invalid_json() {
        let herdr = client(MockCommandRunner::new(&["not json"]));

        let err = herdr.new_window(&cfg(), 0, false).unwrap_err();

        assert_eq!(err.to_string(), "herdr returned invalid JSON");
        herdr
            .runner
            .assert_calls(&[("herdr", CREATE_CALL.to_vec())]);
    }

    #[test]
    fn test_new_pane_stops_when_the_split_response_carries_no_pane_id() {
        let herdr = client(MockCommandRunner::new(&[
            r#"{"id":"cli:pane:split","result":{"type":"pane_info"}}"#,
        ]));

        let err = herdr.new_pane(&cfg(), 1, false).unwrap_err();

        assert_eq!(
            err.to_string(),
            "herdr response carries no .result.pane.pane_id"
        );
        herdr
            .runner
            .assert_calls(&[("herdr", split_call("w0:p1", "right"))]);
    }
}
