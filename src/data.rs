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

/// List archived changes by reading `openspec/changes/archive/` directory entries.
///
/// Each subdirectory becomes a `ChangeEntry` with task progress parsed from its `tasks.md`.
/// Results are sorted by date descending (newest first), then name ascending.
/// Returns an empty list if the archive directory doesn't exist.
pub fn list_archived_changes() -> Result<Vec<ChangeEntry>, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get cwd: {e}"))?;
    let archive_dir = cwd.join("openspec").join("changes").join("archive");

    if !archive_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&archive_dir)
        .map_err(|e| format!("Failed to read archive directory: {e}"))?;

    let mut changes: Vec<ChangeEntry> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let tasks_path = path.join("tasks.md");
            let (completed_tasks, total_tasks) = if tasks_path.exists() {
                parse_task_progress(&tasks_path).unwrap_or((0, 0))
            } else {
                (0, 0)
            };
            Some(ChangeEntry {
                name,
                completed_tasks,
                total_tasks,
                status: "archived".to_string(),
            })
        })
        .collect();

    // Sort: date descending (newest first), then name ascending within same date
    changes.sort_by(|a, b| {
        let date_a = if a.name.len() >= 10 { &a.name[..10] } else { &a.name };
        let date_b = if b.name.len() >= 10 { &b.name[..10] } else { &b.name };
        // Primary: date descending
        let date_cmp = date_b.cmp(date_a);
        if date_cmp != std::cmp::Ordering::Equal {
            return date_cmp;
        }
        // Secondary: name remainder ascending
        let rest_a = if a.name.len() > 11 { &a.name[11..] } else { "" };
        let rest_b = if b.name.len() > 11 { &b.name[11..] } else { "" };
        rest_a.cmp(rest_b)
    });

    Ok(changes)
}

/// Build a `ChangeStatusOutput` for an archived change by checking file existence.
///
/// Instead of calling `openspec status`, checks which artifact files exist in the
/// archive directory. All existing files are treated as "done".
pub fn get_archived_change_status(change_dir: &Path) -> ChangeStatusOutput {
    let change_name = change_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let artifact_checks = [
        ("proposal", "proposal.md"),
        ("design", "design.md"),
        ("tasks", "tasks.md"),
    ];

    let mut artifacts: Vec<ArtifactStatus> = artifact_checks
        .iter()
        .map(|(id, filename)| {
            let status = if change_dir.join(filename).exists() {
                "done"
            } else {
                "pending"
            };
            ArtifactStatus {
                id: id.to_string(),
                output_path: filename.to_string(),
                status: status.to_string(),
            }
        })
        .collect();

    // Check specs directory
    let specs_dir = change_dir.join("specs");
    let specs_status = if specs_dir.exists() && specs_dir.is_dir() {
        "done"
    } else {
        "pending"
    };
    artifacts.push(ArtifactStatus {
        id: "specs".to_string(),
        output_path: "specs/**/*.md".to_string(),
        status: specs_status.to_string(),
    });

    ChangeStatusOutput {
        change_name,
        schema_name: "spec-driven".to_string(),
        artifacts,
    }
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

    // --- list_archived_changes tests ---

    /// Helper: create a temp archive directory structure, run a closure, then clean up.
    fn with_archived_changes<F>(test_name: &str, dirs: &[(&str, Option<&str>)], f: F)
    where
        F: FnOnce(&Path),
    {
        let base = std::env::temp_dir().join(format!("openspec-tui-test-{}", test_name));
        let _ = fs::remove_dir_all(&base);
        let archive_dir = base.join("openspec").join("changes").join("archive");
        fs::create_dir_all(&archive_dir).unwrap();

        for (name, tasks_content) in dirs {
            let change_dir = archive_dir.join(name);
            fs::create_dir_all(&change_dir).unwrap();
            if let Some(content) = tasks_content {
                fs::write(change_dir.join("tasks.md"), content).unwrap();
            }
        }

        // Change to the temp base dir so list_archived_changes() finds the archive
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&base).unwrap();
        f(&base);
        std::env::set_current_dir(&original_dir).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_list_archived_changes_with_changes() {
        with_archived_changes(
            "list-with-changes",
            &[
                ("2026-03-06-foo", Some("- [x] Task\n- [ ] Task\n")),
                ("2026-03-03-bar", Some("- [x] A\n- [x] B\n- [x] C\n")),
            ],
            |_| {
                let result = list_archived_changes().unwrap();
                assert_eq!(result.len(), 2);
                // Newest first
                assert_eq!(result[0].name, "2026-03-06-foo");
                assert_eq!(result[0].completed_tasks, 1);
                assert_eq!(result[0].total_tasks, 2);
                assert_eq!(result[1].name, "2026-03-03-bar");
                assert_eq!(result[1].completed_tasks, 3);
                assert_eq!(result[1].total_tasks, 3);
            },
        );
    }

    #[test]
    fn test_list_archived_changes_empty_directory() {
        with_archived_changes("list-empty-dir", &[], |_| {
            let result = list_archived_changes().unwrap();
            assert!(result.is_empty());
        });
    }

    #[test]
    fn test_list_archived_changes_nonexistent_directory() {
        let base = std::env::temp_dir().join("openspec-tui-test-list-nonexistent");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // No openspec/changes/archive/ directory

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&base).unwrap();
        let result = list_archived_changes().unwrap();
        assert!(result.is_empty());
        std::env::set_current_dir(&original_dir).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_list_archived_changes_sort_order() {
        with_archived_changes(
            "list-sort-order",
            &[
                ("2026-03-03-tui-change-viewer", Some("- [x] A\n")),
                ("2026-03-06-foo", Some("- [x] A\n")),
                ("2026-03-03-add-nix-flake", Some("- [x] A\n")),
                ("2026-03-05-bar", Some("- [x] A\n")),
            ],
            |_| {
                let result = list_archived_changes().unwrap();
                assert_eq!(result.len(), 4);
                // Newest first, then alphabetical within same date
                assert_eq!(result[0].name, "2026-03-06-foo");
                assert_eq!(result[1].name, "2026-03-05-bar");
                assert_eq!(result[2].name, "2026-03-03-add-nix-flake");
                assert_eq!(result[3].name, "2026-03-03-tui-change-viewer");
            },
        );
    }

    #[test]
    fn test_list_archived_changes_no_tasks_md() {
        with_archived_changes(
            "list-no-tasks",
            &[("2026-03-06-no-tasks", None)],
            |_| {
                let result = list_archived_changes().unwrap();
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].completed_tasks, 0);
                assert_eq!(result[0].total_tasks, 0);
            },
        );
    }

    // --- get_archived_change_status tests ---

    #[test]
    fn test_get_archived_change_status_all_present() {
        let dir = std::env::temp_dir().join("openspec-tui-test-status-all");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proposal.md"), "proposal").unwrap();
        fs::write(dir.join("design.md"), "design").unwrap();
        fs::write(dir.join("tasks.md"), "tasks").unwrap();
        let specs_dir = dir.join("specs").join("my-spec");
        fs::create_dir_all(&specs_dir).unwrap();
        fs::write(specs_dir.join("spec.md"), "spec").unwrap();

        let status = get_archived_change_status(&dir);
        assert_eq!(status.artifacts.len(), 4);
        assert!(status.artifacts.iter().all(|a| a.status == "done"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_archived_change_status_some_missing() {
        let dir = std::env::temp_dir().join("openspec-tui-test-status-some");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proposal.md"), "proposal").unwrap();
        // No design.md
        fs::write(dir.join("tasks.md"), "tasks").unwrap();
        // No specs/

        let status = get_archived_change_status(&dir);
        let proposal = status.artifacts.iter().find(|a| a.id == "proposal").unwrap();
        assert_eq!(proposal.status, "done");
        let design = status.artifacts.iter().find(|a| a.id == "design").unwrap();
        assert_eq!(design.status, "pending");
        let tasks = status.artifacts.iter().find(|a| a.id == "tasks").unwrap();
        assert_eq!(tasks.status, "done");
        let specs = status.artifacts.iter().find(|a| a.id == "specs").unwrap();
        assert_eq!(specs.status, "pending");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_archived_change_status_specs_with_subdirs() {
        let dir = std::env::temp_dir().join("openspec-tui-test-status-specs");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let specs_dir = dir.join("specs");
        fs::create_dir_all(specs_dir.join("cap-a")).unwrap();
        fs::write(specs_dir.join("cap-a").join("spec.md"), "spec a").unwrap();
        fs::create_dir_all(specs_dir.join("cap-b")).unwrap();
        fs::write(specs_dir.join("cap-b").join("spec.md"), "spec b").unwrap();

        let status = get_archived_change_status(&dir);
        let specs = status.artifacts.iter().find(|a| a.id == "specs").unwrap();
        assert_eq!(specs.status, "done");
        fs::remove_dir_all(&dir).unwrap();
    }
}
