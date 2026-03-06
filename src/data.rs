use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Types for `openspec list --json`
#[derive(Debug, Deserialize, Clone)]
pub struct ChangeListOutput {
    pub changes: Vec<ChangeEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub name: String,
    pub completed_tasks: u32,
    pub total_tasks: u32,
    pub status: String,
}

// Types for `openspec status --change <name> --json`
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChangeStatusOutput {
    pub change_name: String,
    pub schema_name: String,
    pub artifacts: Vec<ArtifactStatus>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStatus {
    pub id: String,
    pub output_path: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct SpecItem {
    pub name: String,
    pub path: PathBuf,
}

/// Construct a `Command` for invoking the `openspec` CLI.
///
/// On Windows, npm-installed tools use `.cmd` wrappers that `Command::new`
/// cannot resolve directly. We use `cmd /C openspec` so that `cmd.exe`
/// handles PATHEXT resolution. On Unix/macOS, invoke the binary directly.
#[cfg(windows)]
fn openspec_command() -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "openspec"]);
    cmd
}

#[cfg(not(windows))]
fn openspec_command() -> Command {
    Command::new("openspec")
}

/// Construct a `Command` for invoking the `claude` CLI.
///
/// Follows the same cross-platform pattern as `openspec_command`.
#[cfg(windows)]
pub fn claude_command() -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "claude"]);
    cmd
}

#[cfg(not(windows))]
pub fn claude_command() -> Command {
    Command::new("claude")
}

/// Run `openspec list --json` and parse the result.
pub fn list_changes() -> Result<ChangeListOutput, String> {
    let output = openspec_command()
        .args(["list", "--json"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "openspec CLI not found on PATH. Please install openspec first.".to_string()
            } else {
                format!("Failed to run openspec: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("openspec list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse openspec list output: {e}"))
}

/// Run `openspec status --change <name> --json` and parse the result.
pub fn get_change_status(name: &str) -> Result<ChangeStatusOutput, String> {
    let output = openspec_command()
        .args(["status", "--change", name, "--json"])
        .output()
        .map_err(|e| format!("Failed to run openspec status: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("openspec status failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse openspec status output: {e}"))
}

/// Read artifact file content from disk.
pub fn read_artifact_content(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))
}

/// Parse a tasks.md file and count completed vs total tasks.
///
/// Counts lines matching `- [x]` (completed) and `- [ ]` (uncompleted).
/// Returns `(completed, total)`.
pub fn parse_task_progress(path: &Path) -> Result<(u32, u32), String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut completed = 0u32;
    let mut total = 0u32;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            completed += 1;
            total += 1;
        } else if trimmed.starts_with("- [ ]") {
            total += 1;
        }
    }
    Ok((completed, total))
}

/// Find the first unchecked task in a tasks.md file.
///
/// Scans all `- [x]`/`- [X]` (checked) and `- [ ]` (unchecked) lines.
/// Returns the 1-based task index and description text of the first unchecked
/// task, or `None` if all tasks are complete or no tasks exist.
pub fn next_unchecked_task(path: &Path) -> Option<(u32, String)> {
    let content = fs::read_to_string(path).ok()?;
    let mut task_index = 0u32;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            task_index += 1;
        } else if trimmed.starts_with("- [ ]") {
            task_index += 1;
            let text = trimmed.trim_start_matches("- [ ]").trim().to_string();
            return Some((task_index, text));
        }
    }
    None
}

/// Discover spec sub-items by listing the `specs/` subdirectory of a change.
pub fn discover_specs(change_dir: &Path) -> Vec<SpecItem> {
    let specs_dir = change_dir.join("specs");
    let Ok(entries) = fs::read_dir(&specs_dir) else {
        return Vec::new();
    };

    let mut specs: Vec<SpecItem> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                let spec_file = path.join("spec.md");
                if spec_file.exists() {
                    Some(SpecItem {
                        name: entry.file_name().to_string_lossy().to_string(),
                        path: spec_file,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    specs.sort_by(|a, b| a.name.cmp(&b.name));
    specs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_change_list_output() {
        let json = r#"{
            "changes": [
                {
                    "name": "tui-change-viewer",
                    "completedTasks": 3,
                    "totalTasks": 21,
                    "lastModified": "2026-03-03T20:56:50.649Z",
                    "status": "in-progress"
                }
            ]
        }"#;

        let result: ChangeListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].name, "tui-change-viewer");
        assert_eq!(result.changes[0].completed_tasks, 3);
        assert_eq!(result.changes[0].total_tasks, 21);
        assert_eq!(result.changes[0].status, "in-progress");
    }

    #[test]
    fn test_parse_change_list_empty() {
        let json = r#"{"changes": []}"#;
        let result: ChangeListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(result.changes.len(), 0);
    }

    #[test]
    fn test_parse_change_status_output() {
        let json = r#"{
            "changeName": "tui-change-viewer",
            "schemaName": "spec-driven",
            "isComplete": true,
            "applyRequires": ["tasks"],
            "artifacts": [
                {
                    "id": "proposal",
                    "outputPath": "proposal.md",
                    "status": "done"
                },
                {
                    "id": "design",
                    "outputPath": "design.md",
                    "status": "done"
                },
                {
                    "id": "specs",
                    "outputPath": "specs/**/*.md",
                    "status": "done"
                },
                {
                    "id": "tasks",
                    "outputPath": "tasks.md",
                    "status": "pending"
                }
            ]
        }"#;

        let result: ChangeStatusOutput = serde_json::from_str(json).unwrap();
        assert_eq!(result.change_name, "tui-change-viewer");
        assert_eq!(result.schema_name, "spec-driven");
        assert_eq!(result.artifacts.len(), 4);
        assert_eq!(result.artifacts[0].id, "proposal");
        assert_eq!(result.artifacts[0].status, "done");
        assert_eq!(result.artifacts[3].id, "tasks");
        assert_eq!(result.artifacts[3].status, "pending");
    }

    #[test]
    fn test_openspec_command_returns_valid_command() {
        let cmd = openspec_command();
        let program = format!("{:?}", cmd.get_program());
        #[cfg(not(windows))]
        assert_eq!(program, "\"openspec\"");
        #[cfg(windows)]
        assert_eq!(program, "\"cmd\"");
    }

    #[test]
    fn test_claude_command_returns_valid_command() {
        let cmd = claude_command();
        let program = format!("{:?}", cmd.get_program());
        #[cfg(not(windows))]
        assert_eq!(program, "\"claude\"");
        #[cfg(windows)]
        assert_eq!(program, "\"cmd\"");
    }

    #[test]
    fn test_parse_change_list_multiple_changes() {
        let json = r#"{
            "changes": [
                {
                    "name": "change-one",
                    "completedTasks": 0,
                    "totalTasks": 5,
                    "lastModified": "2026-03-01T00:00:00Z",
                    "status": "in-progress"
                },
                {
                    "name": "change-two",
                    "completedTasks": 5,
                    "totalTasks": 5,
                    "lastModified": "2026-03-02T00:00:00Z",
                    "status": "complete"
                }
            ]
        }"#;

        let result: ChangeListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(result.changes.len(), 2);
        assert_eq!(result.changes[0].name, "change-one");
        assert_eq!(result.changes[1].name, "change-two");
    }

    #[test]
    fn test_parse_task_progress_mixed() {
        let dir = std::env::temp_dir().join("openspec-tui-test-mixed");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(
            &path,
            "## Tasks\n\n- [x] Task one\n- [ ] Task two\n- [x] Task three\n- [ ] Task four\n",
        )
        .unwrap();
        let (completed, total) = parse_task_progress(&path).unwrap();
        assert_eq!(completed, 2);
        assert_eq!(total, 4);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_task_progress_all_done() {
        let dir = std::env::temp_dir().join("openspec-tui-test-alldone");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "- [x] Task one\n- [x] Task two\n- [x] Task three\n").unwrap();
        let (completed, total) = parse_task_progress(&path).unwrap();
        assert_eq!(completed, 3);
        assert_eq!(total, 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_task_progress_none_done() {
        let dir = std::env::temp_dir().join("openspec-tui-test-nonedone");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "- [ ] Task one\n- [ ] Task two\n").unwrap();
        let (completed, total) = parse_task_progress(&path).unwrap();
        assert_eq!(completed, 0);
        assert_eq!(total, 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_task_progress_no_tasks() {
        let dir = std::env::temp_dir().join("openspec-tui-test-notasks");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "## Tasks\n\nNo tasks here.\n").unwrap();
        let (completed, total) = parse_task_progress(&path).unwrap();
        assert_eq!(completed, 0);
        assert_eq!(total, 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_task_progress_file_not_found() {
        let path = std::env::temp_dir().join("openspec-tui-test-nonexistent/tasks.md");
        let result = parse_task_progress(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_next_unchecked_task_mixed() {
        let dir = std::env::temp_dir().join("openspec-tui-test-next-mixed");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(
            &path,
            "## Tasks\n\n- [x] Task one\n- [ ] Task two\n- [ ] Task three\n",
        )
        .unwrap();
        let result = next_unchecked_task(&path);
        assert_eq!(result, Some((2, "Task two".to_string())));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_next_unchecked_task_all_complete() {
        let dir = std::env::temp_dir().join("openspec-tui-test-next-alldone");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "- [x] Task one\n- [x] Task two\n").unwrap();
        let result = next_unchecked_task(&path);
        assert_eq!(result, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_next_unchecked_task_no_tasks() {
        let dir = std::env::temp_dir().join("openspec-tui-test-next-notasks");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "## Tasks\n\nNo tasks here.\n").unwrap();
        let result = next_unchecked_task(&path);
        assert_eq!(result, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_next_unchecked_task_missing_file() {
        let path = std::env::temp_dir().join("openspec-tui-test-next-missing/tasks.md");
        let result = next_unchecked_task(&path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_next_unchecked_task_first_is_unchecked() {
        let dir = std::env::temp_dir().join("openspec-tui-test-next-first");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.md");
        fs::write(&path, "- [ ] First task\n- [ ] Second task\n").unwrap();
        let result = next_unchecked_task(&path);
        assert_eq!(result, Some((1, "First task".to_string())));
        fs::remove_dir_all(&dir).unwrap();
    }
}
