use std::path::Path;

use serde::{Deserialize, Serialize};

const CONFIG_PATH: &str = "openspec/tui-config.yaml";
const DEFAULT_COMMAND: &str = "claude --print --dangerously-skip-permissions {prompt}";

const DEFAULT_PROMPT: &str = "Before implementing, read the following files for context:\n\
1. openspec/config.yaml — project context and conventions\n\
2. openspec/changes/{name}/proposal.md — change motivation and scope\n\
3. openspec/changes/{name}/design.md — architecture decisions\n\
4. openspec/changes/{name}/specs/ — detailed requirements\n\
5. openspec/specs/ — global project specifications\n\
\n\
Then read openspec/changes/{name}/tasks.md, take the next unfinished task, \
implement this task, verify if the changes are correct, \
and mark the task as completed.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_command")]
    pub command: String,
    #[serde(default = "default_prompt")]
    pub prompt: String,
}

fn default_command() -> String {
    DEFAULT_COMMAND.to_string()
}

fn default_prompt() -> String {
    DEFAULT_PROMPT.to_string()
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            command: default_command(),
            prompt: default_prompt(),
        }
    }
}

impl TuiConfig {
    /// Replace `{name}` in the prompt template with the given change name.
    pub fn render_prompt(&self, name: &str) -> String {
        self.prompt.replace("{name}", name)
    }

    /// Replace `{prompt}` in the command template, split on whitespace, and
    /// return `(binary, args)`. Returns `None` if the command template is empty.
    pub fn build_command(&self, prompt: &str) -> Option<(String, Vec<String>)> {
        let parts: Vec<String> = self
            .command
            .split_whitespace()
            .map(|token| {
                if token.contains("{prompt}") {
                    token.replace("{prompt}", prompt)
                } else {
                    token.to_string()
                }
            })
            .collect();
        let (first, rest) = parts.split_first()?;
        Some((first.clone(), rest.to_vec()))
    }

    /// Load config from `openspec/tui-config.yaml`. Falls back to defaults if file is missing.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_from(Path::new(CONFIG_PATH))
    }

    /// Load config from a specific path. Falls back to defaults if file is missing.
    pub fn load_from(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let config: TuiConfig = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    /// Save config to `openspec/tui-config.yaml`.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.save_to(Path::new(CONFIG_PATH))
    }

    /// Save config to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_command() {
        let config = TuiConfig::default();
        assert_eq!(
            config.command,
            "claude --print --dangerously-skip-permissions {prompt}"
        );
    }

    #[test]
    fn test_default_prompt_contains_name_placeholder() {
        let config = TuiConfig::default();
        assert!(
            config.prompt.contains("{name}"),
            "default prompt should contain {{name}} placeholder"
        );
    }

    #[test]
    fn test_default_prompt_contains_context_references() {
        let config = TuiConfig::default();
        assert!(config.prompt.contains("openspec/config.yaml"));
        assert!(config.prompt.contains("proposal.md"));
        assert!(config.prompt.contains("design.md"));
        assert!(config.prompt.contains("specs/"));
        assert!(config.prompt.contains("tasks.md"));
    }

    #[test]
    fn test_clone() {
        let config = TuiConfig::default();
        let cloned = config.clone();
        assert_eq!(config.command, cloned.command);
        assert_eq!(config.prompt, cloned.prompt);
    }

    #[test]
    fn test_deserialize_partial_uses_defaults() {
        let yaml = "command: custom-tool {prompt}\n";
        let config: TuiConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.command, "custom-tool {prompt}");
        assert_eq!(config.prompt, DEFAULT_PROMPT);
    }

    #[test]
    fn test_deserialize_empty_uses_defaults() {
        let yaml = "{}";
        let config: TuiConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.command, DEFAULT_COMMAND);
        assert_eq!(config.prompt, DEFAULT_PROMPT);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = TuiConfig {
            command: "my-tool {prompt}".to_string(),
            prompt: "do {name} stuff".to_string(),
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        let deserialized: TuiConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config.command, deserialized.command);
        assert_eq!(config.prompt, deserialized.prompt);
    }

    mod placeholder_tests {
        use super::*;

        #[test]
        fn test_render_prompt_replaces_name() {
            let config = TuiConfig {
                command: default_command(),
                prompt: "implement {name} now".to_string(),
            };
            assert_eq!(config.render_prompt("my-change"), "implement my-change now");
        }

        #[test]
        fn test_render_prompt_replaces_all_occurrences() {
            let config = TuiConfig {
                command: default_command(),
                prompt: "{name}/proposal.md and {name}/tasks.md".to_string(),
            };
            assert_eq!(
                config.render_prompt("feat"),
                "feat/proposal.md and feat/tasks.md"
            );
        }

        #[test]
        fn test_render_prompt_no_placeholder() {
            let config = TuiConfig {
                command: default_command(),
                prompt: "no placeholder here".to_string(),
            };
            assert_eq!(config.render_prompt("x"), "no placeholder here");
        }

        #[test]
        fn test_render_prompt_default_substitutes_correctly() {
            let config = TuiConfig::default();
            let rendered = config.render_prompt("add-auth");
            assert!(rendered.contains("openspec/changes/add-auth/proposal.md"));
            assert!(rendered.contains("openspec/changes/add-auth/tasks.md"));
            assert!(!rendered.contains("{name}"));
        }

        #[test]
        fn test_build_command_basic() {
            let config = TuiConfig {
                command: "claude --print {prompt}".to_string(),
                prompt: default_prompt(),
            };
            let (bin, args) = config.build_command("do stuff").unwrap();
            assert_eq!(bin, "claude");
            assert_eq!(args, vec!["--print", "do stuff"]);
        }

        #[test]
        fn test_build_command_default() {
            let config = TuiConfig::default();
            let (bin, args) = config.build_command("hello world").unwrap();
            assert_eq!(bin, "claude");
            assert_eq!(
                args,
                vec!["--print", "--dangerously-skip-permissions", "hello world"]
            );
        }

        #[test]
        fn test_build_command_custom_tool() {
            let config = TuiConfig {
                command: "aider --message {prompt}".to_string(),
                prompt: default_prompt(),
            };
            let (bin, args) = config.build_command("fix bug").unwrap();
            assert_eq!(bin, "aider");
            assert_eq!(args, vec!["--message", "fix bug"]);
        }

        #[test]
        fn test_build_command_no_prompt_placeholder() {
            let config = TuiConfig {
                command: "my-tool --flag --verbose".to_string(),
                prompt: default_prompt(),
            };
            let (bin, args) = config.build_command("ignored").unwrap();
            assert_eq!(bin, "my-tool");
            assert_eq!(args, vec!["--flag", "--verbose"]);
        }

        #[test]
        fn test_build_command_empty_returns_none() {
            let config = TuiConfig {
                command: "".to_string(),
                prompt: default_prompt(),
            };
            assert!(config.build_command("test").is_none());
        }

        #[test]
        fn test_build_command_single_binary() {
            let config = TuiConfig {
                command: "my-script".to_string(),
                prompt: default_prompt(),
            };
            let (bin, args) = config.build_command("test").unwrap();
            assert_eq!(bin, "my-script");
            assert!(args.is_empty());
        }
    }

    mod load_save_tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_load_missing_file_returns_defaults() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("nonexistent.yaml");
            let config = TuiConfig::load_from(&path).unwrap();
            assert_eq!(config.command, DEFAULT_COMMAND);
            assert_eq!(config.prompt, DEFAULT_PROMPT);
        }

        #[test]
        fn test_load_full_file() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("config.yaml");
            fs::write(&path, "command: my-tool {prompt}\nprompt: do {name}\n").unwrap();
            let config = TuiConfig::load_from(&path).unwrap();
            assert_eq!(config.command, "my-tool {prompt}");
            assert_eq!(config.prompt, "do {name}");
        }

        #[test]
        fn test_load_partial_fields_uses_defaults() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("config.yaml");
            fs::write(&path, "command: custom {prompt}\n").unwrap();
            let config = TuiConfig::load_from(&path).unwrap();
            assert_eq!(config.command, "custom {prompt}");
            assert_eq!(config.prompt, DEFAULT_PROMPT);
        }

        #[test]
        fn test_save_creates_file() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("subdir").join("config.yaml");
            let config = TuiConfig {
                command: "test-tool {prompt}".to_string(),
                prompt: "test prompt {name}".to_string(),
            };
            config.save_to(&path).unwrap();
            assert!(path.exists());
        }

        #[test]
        fn test_save_and_load_roundtrip() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("config.yaml");
            let config = TuiConfig {
                command: "my-cli --flag {prompt}".to_string(),
                prompt: "implement {name} please".to_string(),
            };
            config.save_to(&path).unwrap();
            let loaded = TuiConfig::load_from(&path).unwrap();
            assert_eq!(config.command, loaded.command);
            assert_eq!(config.prompt, loaded.prompt);
        }
    }
}
