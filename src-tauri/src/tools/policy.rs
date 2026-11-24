use std::collections::HashSet;
use std::path::{Component, Path};

use serde_json::Value;

use crate::tools::exec::command_line::split_command;
use crate::tools::exec::paths::{contains_external_path, resolve_workdir};
use crate::tools::workspace::Workspace;

static NETWORK_COMMAND_PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
static DANGEROUS_COMMAND_PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
static INTERPRETER_MUTATION_PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

const MAX_ENV_VARS: usize = 64;
const MAX_ENV_VALUE_BYTES: usize = 8 * 1024;
const FORBIDDEN_ENV_KEYS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
];

const BASIC_READ_ONLY_COMMANDS: &[&str] = &[
    "pwd", "ls", "dir", "cat", "head", "tail", "grep", "find", "which", "echo",
];

const DEFAULT_ALLOWED_COMMANDS: &[&str] = &[
    "pytest",
    "python",
    "python3",
    "npm",
    "npx",
    "node",
    "pnpm",
    "yarn",
    "make",
    "mvn",
    "mvnw",
    "gradle",
    "gradlew",
    "cargo",
    "go",
    "ruff",
    "mypy",
    "eslint",
    "tsc",
    "msbuild",
    "dotnet",
    "deno",
    "bun",
    "ruby",
    "java",
    "javac",
    "cmake",
    "clang",
    "gcc",
    "g++",
    "git",
    "cmd",
    "powershell",
    "pwsh",
];

#[derive(Debug, Clone)]
pub struct PolicySettings {
    pub allowed_commands: HashSet<String>,
    pub workspace_local_entries: bool,
    pub workspace_script_extensions: HashSet<String>,
    pub allow_high_risk_writes: bool,
    pub max_patch_bytes: usize,
    pub permission_mode: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionMode(String);

impl PermissionMode {
    pub fn trusted() -> Self {
        Self("trusted".into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn network_allowed(&self) -> bool {
        matches!(self.as_str(), "trusted" | "dangerous")
    }

    pub fn skip_permission_gates(&self) -> bool {
        self.as_str() == "dangerous"
    }
}

impl From<String> for PermissionMode {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for PermissionMode {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl Default for PolicySettings {
    fn default() -> Self {
        Self {
            allowed_commands: default_allowed_command_set(),
            workspace_local_entries: true,
            workspace_script_extensions: default_workspace_script_extension_set(),
            allow_high_risk_writes: false,
            max_patch_bytes: 200_000,
            permission_mode: PermissionMode::trusted(),
        }
    }
}

impl PolicySettings {
    pub fn from_runtime(runtime: &crate::workspace::RuntimeConfig) -> Self {
        Self {
            allowed_commands: merge_default_allowed_commands(&runtime.allowed_commands),
            workspace_local_entries: runtime.workspace_local_entries,
            workspace_script_extensions: parse_workspace_script_extensions(
                &runtime.workspace_script_extensions,
            ),
            allow_high_risk_writes: runtime.allow_high_risk_writes,
            max_patch_bytes: 200_000,
            permission_mode: runtime.permission_mode.clone().into(),
        }
    }

    pub fn from_runtime_and_global(
        runtime: &crate::workspace::RuntimeConfig,
        global: &crate::settings::AppSettings,
    ) -> Self {
        if !runtime.inherit_global_execution_policy {
            return Self::from_runtime(runtime);
        }

        Self {
            allowed_commands: merge_default_allowed_commands(&global.global_allowed_commands),
            workspace_local_entries: runtime.workspace_local_entries,
            workspace_script_extensions: parse_workspace_script_extensions(
                &runtime.workspace_script_extensions,
            ),
            allow_high_risk_writes: runtime.allow_high_risk_writes,
            max_patch_bytes: 200_000,
            permission_mode: global.global_permission_mode.clone().into(),
        }
    }

    pub fn network_allowed(&self) -> bool {
        self.permission_mode.network_allowed()
    }

    pub fn skip_permission_gates(&self) -> bool {
        self.permission_mode.skip_permission_gates()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("PROTECTED_REPOSITORY_ASSET: {0}")]
    ProtectedRepositoryAsset(String),
    #[error("DANGEROUS_OPERATION_REQUIRES_CONFIRMATION: {0}")]
    ConfirmationRequired(String),
    #[error("{0}")]
    CommandNotAllowlisted(String),
    #[error("{0}")]
    ShellSyntaxRejected(String),
    #[error("{0}")]
    Rejected(String),
}

pub fn parse_allowed_commands(configured: &str) -> HashSet<String> {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return default_allowed_command_set();
    }
    let mut commands: HashSet<String> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    // 基础诊断命令是工作区可用性的最低保障，不应因自定义配置遗漏而失效。
    commands.extend(BASIC_READ_ONLY_COMMANDS.iter().map(|s| s.to_string()));
    commands
}

pub fn parse_workspace_script_extensions(configured: &str) -> HashSet<String> {
    let mut extensions = configured
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with('.') {
                value.to_ascii_lowercase()
            } else {
                format!(".{}", value.to_ascii_lowercase())
            }
        })
        .collect::<HashSet<_>>();
    if extensions.is_empty() {
        extensions = default_workspace_script_extension_set();
    }
    extensions
}

fn default_allowed_command_set() -> HashSet<String> {
    DEFAULT_ALLOWED_COMMANDS
        .iter()
        .map(|s| s.to_string())
        .chain(BASIC_READ_ONLY_COMMANDS.iter().map(|s| s.to_string()))
        .collect()
}

fn merge_default_allowed_commands(configured: &str) -> HashSet<String> {
    let mut commands = default_allowed_command_set();
    commands.extend(parse_allowed_commands(configured));
    commands
}

fn default_workspace_script_extension_set() -> HashSet<String> {
    [".exe", ".bat", ".cmd", ".ps1"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub fn validate_tool_arguments(
    tool_name: &str,
    arguments: &Value,
    policy: &PolicySettings,
) -> Result<(), PolicyError> {
    validate_tool_arguments_for_workspace(tool_name, arguments, policy, None)
}

pub fn validate_tool_arguments_for_workspace(
    tool_name: &str,
    arguments: &Value,
    policy: &PolicySettings,
    workspace: Option<&Workspace>,
) -> Result<(), PolicyError> {
    match tool_name {
        "exec_command" if is_quality_gate_request(arguments) => {
            validate_exec_boundary(arguments, workspace)
        }
        "exec_command" => validate_command_for_workspace(arguments, policy, workspace),
        "apply_patch" | "patch_check" => validate_patch(arguments, policy),
        _ => Ok(()),
    }
}

pub fn validate_command(arguments: &Value, policy: &PolicySettings) -> Result<(), PolicyError> {
    validate_command_for_workspace(arguments, policy, None)
}

pub fn validate_command_for_workspace(
    arguments: &Value,
    policy: &PolicySettings,
    workspace: Option<&Workspace>,
) -> Result<(), PolicyError> {
    validate_exec_boundary(arguments, workspace)?;
    let command = arguments
        .get("cmd")
        .and_then(Value::as_str)
        .ok_or_else(|| PolicyError::Rejected("exec_command requires a non-empty cmd".into()))?;
    if command.trim().is_empty() {
        return Err(PolicyError::Rejected(
            "exec_command requires a non-empty cmd".into(),
        ));
    }
    if command.len() > 4_000 {
        return Err(PolicyError::Rejected("Command is too long".into()));
    }
    let parts = split_command(command).map_err(|message| PolicyError::Rejected(message.into()))?;
    if parts.is_empty() {
        return Err(PolicyError::Rejected("Empty command".into()));
    }
    if has_forbidden_shell_syntax(command) {
        return Err(PolicyError::ShellSyntaxRejected(
            "Shell chaining, redirection and expansion are not allowed".into(),
        ));
    }
    let is_dangerous = dangerous_command_pattern().is_match(command);
    let is_interpreter_mutation = interpreter_mutation_pattern().is_match(command);
    if (is_dangerous || is_interpreter_mutation)
        && command_targets_protected_repository_asset(command)
    {
        return Err(PolicyError::ProtectedRepositoryAsset(
            "禁止删除或递归清空 .git/.github".into(),
        ));
    }
    if is_interpreter_mutation && contains_external_path(&parts[1..], workspace) {
        return Err(PolicyError::Rejected(
            "WORKSPACE_PATH_PROTECTED: workspace scope 禁止通过子进程写入 Workspace 外部路径"
                .into(),
        ));
    }
    if is_dangerous
        && !arguments
            .get("confirm")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(PolicyError::ConfirmationRequired(
            "dangerous command requires confirm=true".into(),
        ));
    }
    if !policy.skip_permission_gates() {
        if network_command_pattern().is_match(command) && !policy.network_allowed() {
            return Err(PolicyError::Rejected(
                "Network-looking commands are blocked in safe permission mode".into(),
            ));
        }

        let executable = parts[0].trim_start_matches("./");
        let base_name = executable.rsplit(['/', '\\']).next().unwrap_or(executable);

        let workspace_entry_candidate =
            workspace_local_entry_exists(workspace, arguments, executable)
                || executable.contains(['/', '\\'])
                || policy
                    .workspace_script_extensions
                    .iter()
                    .any(|extension| base_name.to_ascii_lowercase().ends_with(extension));
        if !(is_allowlisted_program(policy, base_name)
            || (policy.workspace_local_entries && workspace_entry_candidate))
        {
            let stem = strip_windows_executable_suffix(base_name);
            return Err(PolicyError::CommandNotAllowlisted(format!(
                "Command is not allowlisted: {stem}"
            )));
        }

        if arguments.get("env").is_some() {
            return Err(PolicyError::Rejected(
                "Environment variables cannot be supplied unless permission_mode is dangerous"
                    .into(),
            ));
        }
    } else if let Some(env_value) = arguments.get("env") {
        validate_env_argument(env_value)?;
    }

    if let Some(timeout_ms) = arguments.get("timeout_ms").and_then(Value::as_u64) {
        if timeout_ms > 600_000 {
            return Err(PolicyError::Rejected(
                "Command timeout exceeds 10 minutes".into(),
            ));
        }
    }

    Ok(())
}

pub(crate) fn is_allowlisted_program(policy: &PolicySettings, base_name: &str) -> bool {
    let stem = strip_windows_executable_suffix(base_name);
    if policy.allowed_commands.contains(stem) {
        return true;
    }
    // Allow versioned Python binaries (e.g. `python3.13`) whenever `python3` is allowlisted.
    stem.strip_prefix("python3.").is_some_and(|version| {
        !version.is_empty()
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && policy.allowed_commands.contains("python3")
    })
}

fn strip_windows_executable_suffix(name: &str) -> &str {
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".cmd"))
        .or_else(|| name.strip_suffix(".bat"))
        .unwrap_or(name)
}

fn validate_env_argument(env_value: &Value) -> Result<(), PolicyError> {
    let Some(map) = env_value.as_object() else {
        return Err(PolicyError::Rejected(
            "env must be a JSON object of string values".into(),
        ));
    };
    if map.len() > MAX_ENV_VARS {
        return Err(PolicyError::Rejected(format!(
            "Too many environment variables (max {MAX_ENV_VARS})"
        )));
    }
    for (key, value) in map {
        if key.is_empty() || key.contains('=') || key.contains('\0') {
            return Err(PolicyError::Rejected(format!(
                "Invalid environment variable name: {key}"
            )));
        }
        let upper = key.to_ascii_uppercase();
        if FORBIDDEN_ENV_KEYS.contains(&upper.as_str()) {
            return Err(PolicyError::Rejected(format!(
                "Protected environment variable cannot be overridden: {key}"
            )));
        }
        let Some(text) = value.as_str() else {
            return Err(PolicyError::Rejected(format!(
                "Environment variable `{key}` must be a string"
            )));
        };
        if text.contains('\0') || text.len() > MAX_ENV_VALUE_BYTES {
            return Err(PolicyError::Rejected(format!(
                "Environment variable `{key}` value is invalid or exceeds {MAX_ENV_VALUE_BYTES} bytes"
            )));
        }
    }
    Ok(())
}

fn is_quality_gate_request(arguments: &Value) -> bool {
    arguments.get("action").and_then(Value::as_str) == Some("quality_gate")
        || arguments.get("preset").and_then(Value::as_str) == Some("quality_gate")
}

fn validate_exec_boundary(
    arguments: &Value,
    workspace: Option<&Workspace>,
) -> Result<(), PolicyError> {
    let filesystem_scope = arguments
        .get("filesystem_scope")
        .and_then(Value::as_str)
        .unwrap_or("workspace");
    if filesystem_scope != "workspace" {
        return Err(PolicyError::Rejected(
            "EXTERNAL_EXECUTION_NOT_ALLOWED: exec_command 只允许在 Workspace 内执行".into(),
        ));
    }
    for key in ["workdir", "cwd"] {
        if let Some(workdir) = arguments.get(key).and_then(Value::as_str) {
            let path = Path::new(workdir);
            if path.components().any(|part| part == Component::ParentDir)
                || (path.is_absolute() && workspace.is_none())
            {
                return Err(PolicyError::Rejected(
                    "workdir must stay inside the configured workspace".into(),
                ));
            }
            if let Some(workspace) = workspace {
                resolve_workdir(workspace, workdir)
                    .map_err(|err| PolicyError::Rejected(err.message()))?;
            }
        }
    }
    Ok(())
}

fn workspace_local_entry_exists(
    workspace: Option<&Workspace>,
    arguments: &Value,
    executable: &str,
) -> bool {
    let Some(workspace) = workspace else {
        return false;
    };
    let workdir = arguments
        .get("workdir")
        .or_else(|| arguments.get("cwd"))
        .and_then(Value::as_str)
        .unwrap_or(".");
    let Ok(base) = resolve_workdir(workspace, workdir) else {
        return false;
    };
    let candidate = if Path::new(executable).is_absolute() {
        Path::new(executable).to_path_buf()
    } else {
        base.path.join(executable)
    };
    candidate
        .canonicalize()
        .map(|path| path.is_file() && path.starts_with(workspace.root()))
        .unwrap_or(false)
}

pub fn validate_patch(arguments: &Value, policy: &PolicySettings) -> Result<(), PolicyError> {
    let patch = arguments
        .get("patch")
        .and_then(Value::as_str)
        .ok_or_else(|| PolicyError::Rejected("apply_patch requires a patch".into()))?;
    if patch.trim().is_empty() {
        return Err(PolicyError::Rejected("apply_patch requires a patch".into()));
    }

    if patch.len() > policy.max_patch_bytes {
        return Err(PolicyError::Rejected("Patch is too large".into()));
    }

    Ok(())
}

#[derive(Copy, Clone)]
enum ShellQuote {
    Single,
    Double,
}

fn has_forbidden_shell_syntax(command: &str) -> bool {
    if command.contains(['\r', '\n']) {
        return true;
    }

    let chars: Vec<char> = command.chars().collect();
    let mut quote: Option<ShellQuote> = None;
    let mut escaped = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }

        match quote {
            Some(ShellQuote::Single) => {
                if ch == '\'' {
                    quote = None;
                }
            }
            Some(ShellQuote::Double) => {
                if ch == '\\' {
                    // In Windows paths, `\` only escapes `"` (or another `\` before `"`).
                    if !cfg!(windows) || matches!(chars.get(index + 1), Some('"' | '\\')) {
                        escaped = true;
                    }
                } else if ch == '"' {
                    quote = None;
                }
            }
            None => {
                if ch == '\\' && !cfg!(windows) {
                    escaped = true;
                } else if ch == '\'' && !cfg!(windows) {
                    quote = Some(ShellQuote::Single);
                } else if ch == '"' {
                    quote = Some(ShellQuote::Double);
                } else if matches!(ch, ';' | '&' | '|' | '>' | '<' | '`')
                    || (ch == '$'
                        && chars
                            .get(index + 1)
                            .is_some_and(|next| *next == '(' || *next == '{'))
                {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

fn network_command_pattern() -> &'static regex::Regex {
    NETWORK_COMMAND_PATTERN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(https?://|urllib\.request|requests\.|http\.client|\bcurl\b|\bwget\b|\bssh\b|\bscp\b|\bftp\b)",
        )
        .expect("valid regex")
    })
}

fn dangerous_command_pattern() -> &'static regex::Regex {
    DANGEROUS_COMMAND_PATTERN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(git\s+reset\s+--hard|git\s+clean\s+-[^\r\n]*f|git\s+checkout\s+--\s+\.|(^|\s)rm\s+(-[^\r\n]*r[^\r\n]*f|--recursive)|remove-item\s+[^\r\n]*-recurse|(^|\s)(rmdir|del)\s+/s\b)",
        )
        .expect("valid regex")
    })
}

fn interpreter_mutation_pattern() -> &'static regex::Regex {
    INTERPRETER_MUTATION_PATTERN.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(shutil\.(rmtree|move)|os\.(remove|unlink|rmdir)|pathlib\.[^\s;]+\.(unlink|rename)|write_text|write_bytes|fs\.(writefile|writefilesync|unlink|rm)|set-content|out-file|new-item|files?\.(write|delete)|open\([^)]*['\"]w)"#,
        )
        .expect("valid regex")
    })
}

fn command_targets_protected_repository_asset(command: &str) -> bool {
    let normalized_command = command.to_ascii_lowercase().replace('\\', "/");
    let references_protected_asset =
        normalized_command.contains(".git") || normalized_command.contains(".github");
    if !references_protected_asset {
        return false;
    }

    let mutating_operation = [
        "rm ",
        "remove-item",
        "rmdir",
        "del ",
        "unlink",
        "rmtree",
        "write_text",
        "writefile",
        "rename",
        "move",
        "checkout",
        "clean ",
    ]
    .iter()
    .any(|needle| normalized_command.contains(needle));
    if mutating_operation {
        return true;
    }

    command.split_whitespace().any(|part| {
        let token = part
            .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`' | ',' | ';'))
            .replace('\\', "/");
        let token = token.strip_prefix("./").unwrap_or(&token);
        token == ".git"
            || token.starts_with(".git/")
            || token == ".github"
            || token.starts_with(".github/")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn workspace_allowed_commands_override_defaults() {
        let runtime = crate::workspace::RuntimeConfig {
            allowed_commands: "cargo,go".into(),
            ..crate::workspace::RuntimeConfig::default()
        };
        let policy = PolicySettings::from_runtime(&runtime);
        assert!(policy.allowed_commands.contains("cargo"));
        assert!(policy.allowed_commands.contains("pytest"));
    }

    #[test]
    fn trusted_mode_accepts_any_configured_workspace_script_extension() {
        let policy = PolicySettings {
            workspace_local_entries: true,
            workspace_script_extensions: parse_workspace_script_extensions(".cmd,.launcher"),
            ..PolicySettings::default()
        };
        assert!(
            validate_command(&serde_json::json!({ "cmd": "anything.launcher" }), &policy).is_ok()
        );
        assert!(validate_command(
            &serde_json::json!({ "cmd": "scripts/another-name.cmd" }),
            &policy
        )
        .is_ok());
    }

    #[test]
    fn trusted_mode_accepts_an_extensionless_workspace_entry() {
        let dir = tempfile::tempdir().expect("workspace");
        std::fs::write(dir.path().join("project-entry"), "#!/bin/sh\necho ok\n").expect("entry");
        let workspace = Workspace::new(dir.path().to_path_buf()).expect("workspace");
        assert!(validate_command_for_workspace(
            &serde_json::json!({ "cmd": "project-entry", "workdir": "." }),
            &PolicySettings::default(),
            Some(&workspace),
        )
        .is_ok());
    }

    #[test]
    fn patch_size_uses_workspace_limit() {
        let policy = PolicySettings {
            max_patch_bytes: 10,
            ..PolicySettings::default()
        };
        let err = validate_patch(&json!({ "patch": "01234567890" }), &policy).unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn basic_diagnostic_commands_are_allowed() {
        let policy = PolicySettings::default();
        for command in BASIC_READ_ONLY_COMMANDS {
            validate_command(&json!({"cmd": command}), &policy)
                .unwrap_or_else(|err| panic!("{command} should be allowed: {err}"));
        }
    }

    #[test]
    fn configured_commands_keep_basic_diagnostics() {
        let runtime = crate::workspace::RuntimeConfig {
            allowed_commands: "cargo,go".into(),
            ..crate::workspace::RuntimeConfig::default()
        };
        let policy = PolicySettings::from_runtime(&runtime);
        assert!(validate_command(&json!({"cmd": "pwd"}), &policy).is_ok());
        assert!(validate_command(&json!({"cmd": "pytest"}), &policy).is_ok());
    }

    #[test]
    fn inherited_execution_policy_uses_global_mode_and_commands() {
        let runtime = crate::workspace::RuntimeConfig {
            inherit_global_execution_policy: true,
            permission_mode: "safe".into(),
            allowed_commands: "workspace-only".into(),
            allow_high_risk_writes: true,
            ..crate::workspace::RuntimeConfig::default()
        };
        let global = crate::settings::AppSettings {
            global_permission_mode: "trusted".into(),
            global_allowed_commands: "global-tool".into(),
            ..crate::settings::AppSettings::default()
        };

        let policy = PolicySettings::from_runtime_and_global(&runtime, &global);
        assert_eq!(policy.permission_mode.as_str(), "trusted");
        assert!(policy.allowed_commands.contains("global-tool"));
        assert!(!policy.allowed_commands.contains("workspace-only"));
        assert!(policy.allow_high_risk_writes);
    }

    #[test]
    fn legacy_workspace_keeps_explicit_execution_policy() {
        let runtime = crate::workspace::RuntimeConfig {
            inherit_global_execution_policy: false,
            permission_mode: "safe".into(),
            allowed_commands: "workspace-only".into(),
            ..crate::workspace::RuntimeConfig::default()
        };
        let global = crate::settings::AppSettings {
            global_permission_mode: "dangerous".into(),
            global_allowed_commands: "global-tool".into(),
            ..crate::settings::AppSettings::default()
        };

        let policy = PolicySettings::from_runtime_and_global(&runtime, &global);
        assert_eq!(policy.permission_mode.as_str(), "safe");
        assert!(policy.allowed_commands.contains("workspace-only"));
        assert!(!policy.allowed_commands.contains("global-tool"));
    }

    #[test]
    fn quoted_python_code_is_not_treated_as_shell_chaining() {
        let policy = PolicySettings::default();
        assert!(validate_command(
            &json!({"cmd": "python -c \"import os; print(os.getcwd())\""}),
            &policy
        )
        .is_ok());
        assert!(validate_command(
            &json!({"cmd": "python -c \"print(1)\" && echo nope"}),
            &policy
        )
        .is_err());
        assert!(validate_command(&json!({"cmd": "echo hello > output.txt"}), &policy).is_err());
    }

    #[test]
    fn dangerous_mode_allows_non_allowlisted_commands_and_env_while_keeping_boundaries() {
        let dangerous = PolicySettings {
            permission_mode: PermissionMode::from("dangerous"),
            ..PolicySettings::default()
        };
        assert!(validate_command(
            &json!({
                "cmd": "psql --version",
                "env": { "PGPASSWORD": "secret", "PGHOST": "127.0.0.1" }
            }),
            &dangerous,
        )
        .is_ok());

        // Protected loader hijacking variables are still blocked even in dangerous mode.
        assert!(validate_command(
            &json!({
                "cmd": "psql --version",
                "env": { "LD_PRELOAD": "/tmp/evil.so" }
            }),
            &dangerous,
        )
        .is_err());

        // Shell chaining and traversal remain blocked in dangerous mode.
        assert!(validate_command(&json!({ "cmd": "psql && echo bad" }), &dangerous).is_err());
        assert!(validate_command(&json!({ "cmd": "psql", "workdir": ".." }), &dangerous).is_err());

        // Trusted and safe modes still reject env and non-allowlisted commands.
        let trusted = PolicySettings::default();
        assert!(validate_command(
            &json!({ "cmd": "python3 --version", "env": { "FOO": "bar" } }),
            &trusted,
        )
        .is_err());
        assert!(validate_command(&json!({ "cmd": "psql --version" }), &trusted).is_err());
    }

    #[test]
    fn allowlist_recognizes_versioned_python_and_workspace_absolute_workdir() {
        let dir = tempfile::tempdir().expect("workspace");
        let sub = dir.path().join("pkg");
        std::fs::create_dir_all(&sub).expect("sub");
        let workspace = Workspace::new(dir.path().to_path_buf()).expect("workspace");
        let policy = PolicySettings::default();

        assert!(is_allowlisted_program(&policy, "python3.13"));
        assert!(is_allowlisted_program(&policy, "python3.12.4"));
        assert!(!is_allowlisted_program(&policy, "python3.evil"));

        assert!(validate_command_for_workspace(
            &json!({ "cmd": "python3.13 --version", "workdir": sub.to_string_lossy() }),
            &policy,
            Some(&workspace),
        )
        .is_ok());

        let outside = tempfile::tempdir().expect("outside");
        assert!(validate_command_for_workspace(
            &json!({ "cmd": "python3 --version", "workdir": outside.path().to_string_lossy() }),
            &policy,
            Some(&workspace),
        )
        .is_err());
    }

    #[test]
    fn interpreter_mutation_ignores_executable_path_and_blocks_external_targets() {
        let dir = tempfile::tempdir().expect("workspace");
        let workspace = Workspace::new(dir.path().to_path_buf()).expect("workspace");
        let policy = PolicySettings::default();

        // Absolute interpreter path writing to a relative workspace file is allowed.
        assert!(validate_command_for_workspace(
            &json!({
                "cmd": "/usr/bin/python3 -c \"from pathlib import Path; Path('ok.txt').write_text('x')\""
            }),
            &policy,
            Some(&workspace),
        )
        .is_ok());

        // Writing to an external absolute path is still blocked.
        let outside = tempfile::tempdir().expect("outside");
        let external_target = outside.path().join("escape.txt");
        assert!(validate_command_for_workspace(
            &json!({
                "cmd": format!(
                    "/usr/bin/python3 -c \"from pathlib import Path; Path('{}').write_text('x')\"",
                    external_target.to_string_lossy().replace('\\', "/")
                )
            }),
            &policy,
            Some(&workspace),
        )
        .is_err());
    }
}
