use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use crate::data;

/// Messages sent from the worker thread to the TUI.
pub enum ImplUpdate {
    Progress { completed: u32, total: u32 },
    Finished,
}

/// State for a running implementation process.
pub struct ImplState {
    pub change_name: String,
    pub completed: u32,
    pub total: u32,
    pub log_path: PathBuf,
    pub receiver: Receiver<ImplUpdate>,
    pub cancel_flag: Arc<AtomicBool>,
    pub child_handle: Arc<Mutex<Option<Child>>>,
}

/// Stop a running implementation by setting the cancel flag and killing the
/// active child process.
pub fn stop_implementation(state: &ImplState) {
    state.cancel_flag.store(true, Ordering::Relaxed);
    if let Ok(mut handle) = state.child_handle.lock() {
        if let Some(ref mut child) = *handle {
            let _ = child.kill();
        }
    }
}

/// Start an implementation runner for the given change.
///
/// Spawns a worker thread that loops through unfinished tasks in tasks.md,
/// invoking `claude --print --dangerously-skip-permissions` for each one.
/// Claude output is redirected to a log file. Progress updates are sent
/// via the mpsc channel stored in the returned `ImplState`.
pub fn start_implementation(change_name: &str) -> ImplState {
    let tasks_path = PathBuf::from("openspec/changes")
        .join(change_name)
        .join("tasks.md");
    let log_path = PathBuf::from("openspec/changes")
        .join(change_name)
        .join("implementation.log");

    let (tx, rx) = mpsc::channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    // Read initial progress
    let (completed, total) = data::parse_task_progress(&tasks_path).unwrap_or((0, 0));

    let worker_cancel = cancel_flag.clone();
    let worker_child = child_handle.clone();
    let worker_log_path = log_path.clone();
    let worker_change_name = change_name.to_string();

    std::thread::spawn(move || {
        implementation_loop(
            &worker_change_name,
            &tasks_path,
            &worker_log_path,
            &tx,
            &worker_cancel,
            &worker_child,
        );
    });

    ImplState {
        change_name: change_name.to_string(),
        completed,
        total,
        log_path,
        receiver: rx,
        cancel_flag,
        child_handle,
    }
}

fn build_prompt(change_name: &str) -> String {
    format!(
        "Before implementing, read the following files for context:\n\
         1. openspec/config.yaml — project context and conventions\n\
         2. openspec/changes/{name}/proposal.md — change motivation and scope\n\
         3. openspec/changes/{name}/design.md — architecture decisions\n\
         4. openspec/changes/{name}/specs/ — detailed requirements\n\
         5. openspec/specs/ — global project specifications\n\
         \n\
         Then read openspec/changes/{name}/tasks.md, take the next unfinished task, \
         implement this task, verify if the changes are correct, \
         and mark the task as completed.",
        name = change_name
    )
}

fn write_task_header(
    log_path: &PathBuf,
    task_number: u32,
    total: u32,
    task_text: &str,
) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    writeln!(file, "──────────────────────────────────────────────────────────────")?;
    writeln!(file, "Task {}/{}: {}", task_number, total, task_text)?;
    writeln!(file, "──────────────────────────────────────────────────────────────")?;
    Ok(())
}

fn write_run_header(log_path: &PathBuf, change_name: &str) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "══════════════════════════════════════════════════════════════")?;
    writeln!(file, "IMPLEMENTATION RUN STARTED")?;
    writeln!(file, "Time: {}", timestamp)?;
    writeln!(file, "Change: {}", change_name)?;
    writeln!(file, "══════════════════════════════════════════════════════════════")?;
    Ok(())
}

fn implementation_loop(
    change_name: &str,
    tasks_path: &PathBuf,
    log_path: &PathBuf,
    tx: &mpsc::Sender<ImplUpdate>,
    cancel_flag: &Arc<AtomicBool>,
    child_handle: &Arc<Mutex<Option<Child>>>,
) {
    // Write run header before starting the task loop
    let _ = write_run_header(log_path, change_name);

    loop {
        // Check cancellation
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        // Check if there are unchecked tasks remaining
        let (completed, total) = data::parse_task_progress(tasks_path).unwrap_or((0, 0));
        if completed >= total || total == 0 {
            let _ = tx.send(ImplUpdate::Finished);
            break;
        }

        // Open log file for appending
        let log_file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            Ok(f) => f,
            Err(_) => {
                let _ = tx.send(ImplUpdate::Finished);
                break;
            }
        };
        let stderr_log = match log_file.try_clone() {
            Ok(f) => f,
            Err(_) => {
                let _ = tx.send(ImplUpdate::Finished);
                break;
            }
        };

        // Write task header before spawning claude
        let (_, total) = data::parse_task_progress(tasks_path).unwrap_or((0, 0));
        if let Some((task_num, task_text)) = data::next_unchecked_task(tasks_path) {
            let _ = write_task_header(log_path, task_num, total, &task_text);
        }

        let prompt = build_prompt(change_name);

        // Spawn claude process
        let child_result = data::claude_command()
            .args(["--print", "--dangerously-skip-permissions", &prompt])
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(stderr_log))
            .spawn();

        let child = match child_result {
            Ok(c) => c,
            Err(_) => {
                let _ = tx.send(ImplUpdate::Finished);
                break;
            }
        };

        // Store child handle so main thread can kill it
        {
            let mut handle = child_handle.lock().unwrap();
            *handle = Some(child);
        }

        // Poll for child completion using try_wait so we don't hold the
        // mutex lock. This allows the main thread to lock the mutex and
        // kill the child process for cancellation.
        let exited_ok = loop {
            if cancel_flag.load(Ordering::Relaxed) {
                break false;
            }

            let try_result = {
                let mut handle = child_handle.lock().unwrap();
                if let Some(ref mut c) = *handle {
                    c.try_wait()
                } else {
                    break false;
                }
            };

            match try_result {
                Ok(Some(status)) => break status.success(),
                Ok(None) => {
                    // Process still running, wait briefly before polling again
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => break false,
            }
        };

        // Clear child handle
        {
            let mut handle = child_handle.lock().unwrap();
            *handle = None;
        }

        // If cancelled or process failed, stop
        if cancel_flag.load(Ordering::Relaxed) || !exited_ok {
            let _ = tx.send(ImplUpdate::Finished);
            break;
        }

        // Re-read progress and send update
        let (completed, total) = data::parse_task_progress(tasks_path).unwrap_or((0, 0));
        if tx.send(ImplUpdate::Progress { completed, total }).is_err() {
            break;
        }

        // If all tasks completed, finish
        if completed >= total {
            let _ = tx.send(ImplUpdate::Finished);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_impl_state_creation() {
        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle = Arc::new(Mutex::new(None));

        let state = ImplState {
            change_name: "test-change".to_string(),
            completed: 2,
            total: 5,
            log_path: PathBuf::from("openspec/changes/test-change/implementation.log"),
            receiver: rx,
            cancel_flag: cancel_flag.clone(),
            child_handle: child_handle.clone(),
        };

        assert_eq!(state.change_name, "test-change");
        assert_eq!(state.completed, 2);
        assert_eq!(state.total, 5);
        assert_eq!(
            state.log_path,
            PathBuf::from("openspec/changes/test-change/implementation.log")
        );
        assert!(!state.cancel_flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(state.child_handle.lock().unwrap().is_none());

        // Verify channel works
        tx.send(ImplUpdate::Progress {
            completed: 3,
            total: 5,
        })
        .unwrap();
        let msg = state.receiver.recv().unwrap();
        match msg {
            ImplUpdate::Progress {
                completed,
                total,
            } => {
                assert_eq!(completed, 3);
                assert_eq!(total, 5);
            }
            ImplUpdate::Finished => panic!("Expected Progress, got Finished"),
        }
    }

    #[test]
    fn test_impl_update_finished() {
        let (tx, rx) = mpsc::channel();
        tx.send(ImplUpdate::Finished).unwrap();
        let msg = rx.recv().unwrap();
        assert!(matches!(msg, ImplUpdate::Finished));
    }

    #[test]
    fn test_cancel_flag_shared() {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = cancel_flag.clone();

        // Simulate main thread setting cancel
        flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(cancel_flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_stop_implementation_sets_cancel_flag() {
        let (_, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle = Arc::new(Mutex::new(None));

        let state = ImplState {
            change_name: "test-change".to_string(),
            completed: 0,
            total: 5,
            log_path: PathBuf::from("openspec/changes/test-change/implementation.log"),
            receiver: rx,
            cancel_flag: cancel_flag.clone(),
            child_handle: child_handle.clone(),
        };

        assert!(!cancel_flag.load(Ordering::Relaxed));
        stop_implementation(&state);
        assert!(cancel_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_stop_implementation_kills_child_process() {
        let (_, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle = Arc::new(Mutex::new(None));

        // Spawn a long-running child process to test killing
        let child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep process");
        let child_id = child.id();
        *child_handle.lock().unwrap() = Some(child);

        let state = ImplState {
            change_name: "test-change".to_string(),
            completed: 0,
            total: 5,
            log_path: PathBuf::from("openspec/changes/test-change/implementation.log"),
            receiver: rx,
            cancel_flag: cancel_flag.clone(),
            child_handle: child_handle.clone(),
        };

        stop_implementation(&state);

        // Cancel flag should be set
        assert!(cancel_flag.load(Ordering::Relaxed));

        // Child process should have been killed - wait for it to confirm
        if let Some(ref mut child) = *child_handle.lock().unwrap() {
            let status = child.wait().expect("failed to wait on child");
            assert!(!status.success(), "child should have been killed");
        }

        // Verify process is no longer running (check via /proc on linux)
        assert!(
            !std::path::Path::new(&format!("/proc/{child_id}")).exists(),
            "process should no longer exist"
        );
    }

    #[test]
    fn test_stop_implementation_no_child() {
        // stop_implementation should not panic when there is no child process
        let (_, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

        let state = ImplState {
            change_name: "test-change".to_string(),
            completed: 0,
            total: 5,
            log_path: PathBuf::from("openspec/changes/test-change/implementation.log"),
            receiver: rx,
            cancel_flag: cancel_flag.clone(),
            child_handle,
        };

        stop_implementation(&state);
        assert!(cancel_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_child_handle_shared() {
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let handle_clone = child_handle.clone();

        // Verify both references see the same state
        assert!(handle_clone.lock().unwrap().is_none());
        assert!(child_handle.lock().unwrap().is_none());
    }

    #[test]
    fn test_cancel_flag_stops_loop() {
        // Create a tasks file with uncompleted tasks
        let dir = std::env::temp_dir().join("openspec-tui-test-cancel-loop");
        let change_dir = dir.join("openspec/changes/test-cancel");
        std::fs::create_dir_all(&change_dir).unwrap();
        let tasks_path = change_dir.join("tasks.md");
        std::fs::write(&tasks_path, "- [ ] Task one\n- [ ] Task two\n").unwrap();

        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(true)); // Pre-set cancel
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let log_path = dir.join("test.log");

        // Run the loop — it should exit immediately due to cancel flag
        implementation_loop(
            "test-cancel",
            &tasks_path,
            &log_path,
            &tx,
            &cancel_flag,
            &child_handle,
        );

        // The loop should not have sent any Progress message
        // It may or may not send Finished (implementation breaks before sending),
        // but it must not hang or send Progress.
        let mut got_progress = false;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, ImplUpdate::Progress { .. }) {
                got_progress = true;
            }
        }
        assert!(!got_progress, "Loop should not send Progress when cancelled");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_loop_finishes_when_all_tasks_complete() {
        // Create a tasks file where all tasks are already done
        let dir = std::env::temp_dir().join("openspec-tui-test-all-done-loop");
        let change_dir = dir.join("openspec/changes/test-done");
        std::fs::create_dir_all(&change_dir).unwrap();
        let tasks_path = change_dir.join("tasks.md");
        std::fs::write(&tasks_path, "- [x] Task one\n- [x] Task two\n").unwrap();

        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let log_path = dir.join("test.log");

        implementation_loop(
            "test-done",
            &tasks_path,
            &log_path,
            &tx,
            &cancel_flag,
            &child_handle,
        );

        // Should receive Finished since all tasks are complete
        let msg = rx.recv().unwrap();
        assert!(matches!(msg, ImplUpdate::Finished));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_loop_sends_finished_on_spawn_failure() {
        // Create a tasks file with uncompleted tasks so the loop tries to spawn claude
        let dir = std::env::temp_dir().join("openspec-tui-test-spawn-fail");
        let change_dir = dir.join("openspec/changes/test-spawn");
        std::fs::create_dir_all(&change_dir).unwrap();
        let tasks_path = change_dir.join("tasks.md");
        std::fs::write(&tasks_path, "- [ ] Task one\n").unwrap();

        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        // Use an invalid log path to cause the log file open to fail,
        // or the claude command to fail (claude likely not available in test env)
        let log_path = dir.join("test.log");

        implementation_loop(
            "test-spawn",
            &tasks_path,
            &log_path,
            &tx,
            &cancel_flag,
            &child_handle,
        );

        // Should receive Finished since claude spawn will fail in test environment
        let msg = rx.recv().unwrap();
        assert!(matches!(msg, ImplUpdate::Finished));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_prompt_contains_all_context_references() {
        let prompt = build_prompt("my-change");
        assert!(
            prompt.contains("openspec/config.yaml"),
            "prompt should reference config.yaml"
        );
        assert!(
            prompt.contains("openspec/changes/my-change/proposal.md"),
            "prompt should reference proposal.md"
        );
        assert!(
            prompt.contains("openspec/changes/my-change/design.md"),
            "prompt should reference design.md"
        );
        assert!(
            prompt.contains("openspec/changes/my-change/specs/"),
            "prompt should reference change specs directory"
        );
        assert!(
            prompt.contains("openspec/specs/"),
            "prompt should reference global specs directory"
        );
        assert!(
            prompt.contains("openspec/changes/my-change/tasks.md"),
            "prompt should reference tasks.md"
        );
        assert!(
            !prompt.contains("Library-Constraints"),
            "prompt should not contain hard-coded Library-Constraints"
        );
    }

    #[test]
    fn test_progress_counting_via_channel() {
        // Verify that ImplUpdate::Progress carries correct counts
        let (tx, rx) = mpsc::channel();

        tx.send(ImplUpdate::Progress {
            completed: 3,
            total: 7,
        })
        .unwrap();
        tx.send(ImplUpdate::Progress {
            completed: 5,
            total: 7,
        })
        .unwrap();
        tx.send(ImplUpdate::Finished).unwrap();

        match rx.recv().unwrap() {
            ImplUpdate::Progress { completed, total } => {
                assert_eq!(completed, 3);
                assert_eq!(total, 7);
            }
            _ => panic!("Expected Progress"),
        }
        match rx.recv().unwrap() {
            ImplUpdate::Progress { completed, total } => {
                assert_eq!(completed, 5);
                assert_eq!(total, 7);
            }
            _ => panic!("Expected Progress"),
        }
        assert!(matches!(rx.recv().unwrap(), ImplUpdate::Finished));
    }

    #[test]
    fn test_write_run_header_creates_file_with_content() {
        let dir = std::env::temp_dir().join("openspec-tui-test-run-header");
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("implementation.log");

        write_run_header(&log_path, "my-change").unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("══"), "Should contain separator lines");
        assert!(
            content.contains("IMPLEMENTATION RUN STARTED"),
            "Should contain run started text"
        );
        assert!(content.contains("Time:"), "Should contain timestamp label");
        assert!(
            content.contains("Change: my-change"),
            "Should contain change name"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_run_header_appends_on_second_call() {
        let dir = std::env::temp_dir().join("openspec-tui-test-run-header-append");
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("implementation.log");

        write_run_header(&log_path, "change-a").unwrap();
        write_run_header(&log_path, "change-b").unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("Change: change-a"),
            "First run header should be present"
        );
        assert!(
            content.contains("Change: change-b"),
            "Second run header should be present"
        );
        let count = content.matches("IMPLEMENTATION RUN STARTED").count();
        assert_eq!(count, 2, "Should have two run headers");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_implementation_loop_writes_run_header() {
        let dir = std::env::temp_dir().join("openspec-tui-test-loop-run-header");
        let change_dir = dir.join("openspec/changes/test-header");
        std::fs::create_dir_all(&change_dir).unwrap();
        let tasks_path = change_dir.join("tasks.md");
        std::fs::write(&tasks_path, "- [x] Task one\n- [x] Task two\n").unwrap();
        let log_path = dir.join("test.log");

        let (tx, _rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

        implementation_loop(
            "test-header",
            &tasks_path,
            &log_path,
            &tx,
            &cancel_flag,
            &child_handle,
        );

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("IMPLEMENTATION RUN STARTED"),
            "Run header should be written even when all tasks are complete"
        );
        assert!(
            content.contains("Change: test-header"),
            "Run header should contain change name"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_task_header_creates_file_with_content() {
        let dir = std::env::temp_dir().join("openspec-tui-test-task-header");
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("implementation.log");

        write_task_header(&log_path, 3, 7, "Implement the widget").unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("──"), "Should contain separator lines");
        assert!(
            content.contains("Task 3/7: Implement the widget"),
            "Should contain task number and description"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_task_header_appends() {
        let dir = std::env::temp_dir().join("openspec-tui-test-task-header-append");
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("implementation.log");

        write_task_header(&log_path, 1, 3, "First task").unwrap();
        write_task_header(&log_path, 2, 3, "Second task").unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("Task 1/3: First task"),
            "First task header should be present"
        );
        assert!(
            content.contains("Task 2/3: Second task"),
            "Second task header should be present"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_implementation_loop_writes_task_header() {
        // Create a tasks file with one unchecked task so the loop tries to spawn
        // claude (which will fail in test env), but should still write the task header
        let dir = std::env::temp_dir().join("openspec-tui-test-loop-task-header");
        let change_dir = dir.join("openspec/changes/test-task-hdr");
        std::fs::create_dir_all(&change_dir).unwrap();
        let tasks_path = change_dir.join("tasks.md");
        std::fs::write(&tasks_path, "- [x] Done task\n- [ ] Pending task\n").unwrap();
        let log_path = dir.join("test.log");

        let (tx, _rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let child_handle: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

        implementation_loop(
            "test-task-hdr",
            &tasks_path,
            &log_path,
            &tx,
            &cancel_flag,
            &child_handle,
        );

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("IMPLEMENTATION RUN STARTED"),
            "Run header should be present"
        );
        assert!(
            content.contains("Task 2/2: Pending task"),
            "Task header should contain task number and description"
        );
        assert!(
            content.contains("──"),
            "Task header should contain separator lines"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
