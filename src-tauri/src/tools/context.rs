use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::agent_context::{
    discover_instructions, discover_skills, render_instruction_documents, AgentContextRuntimeConfig,
    SkillEntry,
};
use crate::harness::Harness;
use crate::tools::policy::PolicySettings;
use crate::tools::session::SessionStore;
use crate::tools::workspace::{relative_display, Workspace};
use crate::workspace::AuthConfig;

pub struct ToolContext {
    pub workspace: Workspace,
    pub auth: AuthConfig,
    pub policy: PolicySettings,
    pub tool_profile: String,
    pub permission_mode: String,
    pub executable_paths: Vec<PathBuf>,
    pub ai_instructions: String,
    pub skills: Vec<SkillEntry>,
    pub agent_context: Option<AgentContextRuntimeConfig>,
    pub harness: Harness,
    default_cwd: Mutex<PathBuf>,
    pub sessions: Arc<SessionStore>,
}

pub type SharedToolContext = Arc<ToolContext>;

impl ToolContext {
    pub fn new(workspace_path: PathBuf) -> Result<Self, String> {
        let workspace = Workspace::new(workspace_path).map_err(|e| e.message())?;
        let auth = AuthConfig {
            auth_type: "noauth".into(),
            ..AuthConfig::default()
        };
        Ok(Self::from_workspace(
            workspace,
            auth,
            PolicySettings::default(),
            "full".into(),
            "trusted".into(),
        ))
    }

    pub fn from_workspace(
        workspace: Workspace,
        auth: AuthConfig,
        policy: PolicySettings,
        tool_profile: String,
        permission_mode: String,
    ) -> Self {
        let harness_root = Harness::default_root().expect("无法初始化 Harness 数据目录");
        Self::from_workspace_with_harness_root(
            workspace,
            auth,
            policy,
            crate::tools::registry::normalize_tool_profile(&tool_profile).into(),
            permission_mode,
            harness_root,
        )
    }

    pub fn from_workspace_with_harness_root(
        workspace: Workspace,
        auth: AuthConfig,
        policy: PolicySettings,
        tool_profile: String,
        permission_mode: String,
        harness_root: PathBuf,
    ) -> Self {
        let root = workspace.root().to_path_buf();
        let sessions = Arc::new(SessionStore::new());
        crate::tools::session::register_workspace_session_store(&root, &sessions);
        Self {
            workspace,
            auth,
            policy,
            tool_profile: crate::tools::registry::normalize_tool_profile(&tool_profile).into(),
            permission_mode,
            executable_paths: Vec::new(),
            ai_instructions: String::new(),
            skills: Vec::new(),
            agent_context: None,
            harness: Harness::new(root.clone(), harness_root).expect("无法初始化 Harness"),
            default_cwd: Mutex::new(root),
            sessions,
        }
    }

    pub fn for_test(workspace_path: PathBuf, harness_root: PathBuf) -> Result<Self, String> {
        let workspace = Workspace::new(workspace_path).map_err(|e| e.message())?;
        Ok(Self::from_workspace_with_harness_root(
            workspace,
            AuthConfig {
                auth_type: "noauth".into(),
                ..AuthConfig::default()
            },
            PolicySettings::default(),
            "full".into(),
            "trusted".into(),
            harness_root,
        ))
    }

    pub fn with_agent_runtime(
        mut self,
        executable_paths: Vec<PathBuf>,
        ai_instructions: String,
    ) -> Self {
        self.executable_paths = executable_paths;
        self.ai_instructions = ai_instructions;
        self
    }

    pub fn with_skills(mut self, skills: Vec<SkillEntry>) -> Self {
        self.skills = skills;
        self
    }

    pub fn with_agent_context(mut self, config: AgentContextRuntimeConfig) -> Self {
        self.agent_context = Some(config);
        self
    }

    pub fn current_ai_instructions(&self) -> String {
        let Some(config) = &self.agent_context else {
            return self.ai_instructions.clone();
        };
        let repository_instructions = discover_instructions(
            self.workspace.root(),
            &config.instruction_sources,
            &config.custom_instruction_paths,
        );
        merge_ai_instructions(
            &self.ai_instructions,
            &render_instruction_documents(&repository_instructions),
        )
    }

    pub fn current_skills(&self) -> Vec<SkillEntry> {
        let Some(config) = &self.agent_context else {
            return self.skills.clone();
        };
        discover_skills(
            self.workspace.root(),
            &config.skill_sources,
            &config.custom_skill_paths,
        )
    }

    pub fn executable_path_env(&self) -> Option<OsString> {
        let mut paths = self.executable_paths.clone();
        if let Some(system_path) = std::env::var_os("PATH") {
            for path in std::env::split_paths(&system_path) {
                push_unique_path(&mut paths, path);
            }
        }
        std::env::join_paths(paths).ok()
    }

    pub fn workspace_path(&self) -> String {
        self.workspace.root_display()
    }

    pub fn default_cwd_display(&self) -> String {
        let cwd = self.default_cwd.lock().expect("cwd lock");
        relative_display(self.workspace.root(), &cwd)
    }

    pub fn set_default_cwd(&self, path: PathBuf) {
        *self.default_cwd.lock().expect("cwd lock") = path;
    }

    pub fn default_cwd_path(&self) -> PathBuf {
        self.default_cwd.lock().expect("cwd lock").clone()
    }
}

pub fn merge_executable_paths(workspace_paths: &str, global_paths: &str) -> Vec<PathBuf> {
    let mut output = Vec::new();
    for configured in [workspace_paths, global_paths] {
        for group in configured.split(|c| c == '\n' || c == ',') {
            let group = group.trim();
            if group.is_empty() {
                continue;
            }
            for path in std::env::split_paths(OsStr::new(group)) {
                let value = path.to_string_lossy();
                let value = value.trim();
                if !value.is_empty() {
                    push_unique_path(&mut output, expand_home(value));
                }
            }
        }
    }
    output
}

pub fn merge_ai_instructions(global: &str, workspace: &str) -> String {
    [global.trim(), workspace.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/").or_else(|| value.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(value).to_path_buf()
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_paths_precede_global_paths_and_duplicates_are_removed() {
        let paths = merge_executable_paths(
            "/workspace/bin\n/common/bin",
            "/global/bin\n/common/bin",
        );

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/workspace/bin"),
                PathBuf::from("/common/bin"),
                PathBuf::from("/global/bin"),
            ]
        );
    }

    #[test]
    fn executable_paths_accept_native_path_list_syntax() {
        let joined = std::env::join_paths([PathBuf::from("/first"), PathBuf::from("/second")])
            .expect("join paths")
            .to_string_lossy()
            .into_owned();
        let paths = merge_executable_paths(&joined, "");

        assert_eq!(paths, vec![PathBuf::from("/first"), PathBuf::from("/second")]);
    }

    #[test]
    fn ai_instructions_merge_global_before_workspace() {
        assert_eq!(
            merge_ai_instructions("global rule", "workspace rule"),
            "global rule\n\nworkspace rule"
        );
    }
}
