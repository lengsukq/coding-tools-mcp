use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::workspace::WorkspaceProfile;

#[derive(Debug, Clone, Copy)]
struct ResourceClaim<'a> {
    profile: &'a WorkspaceProfile,
    local_port: u16,
    subdomain: &'a str,
    uses_frp: bool,
}

impl<'a> ResourceClaim<'a> {
    fn from_profile(profile: &'a WorkspaceProfile) -> Self {
        Self {
            profile,
            local_port: profile.runtime.local_port,
            subdomain: profile.tunnel.frp_subdomain.as_str(),
            uses_frp: profile.tunnel.tunnel_type == "frp",
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn validate_workspace_resources(
    profiles: &[WorkspaceProfile],
    candidate: &WorkspaceProfile,
) -> AppResult<()> {
    validate_claim_against_others(profiles, candidate)
}

/// Assign a free MCP port for a newly created workspace.
pub fn assign_free_workspace_ports(
    profiles: &[WorkspaceProfile],
    candidate: &mut WorkspaceProfile,
) -> AppResult<()> {
    let reserved = profiles
        .iter()
        .filter(|profile| profile.id != candidate.id)
        .map(|profile| profile.runtime.local_port)
        .collect::<HashSet<_>>();

    candidate.runtime.local_port = next_free_port(candidate.runtime.local_port, &reserved)?;
    Ok(())
}

fn next_free_port(preferred: u16, reserved: &HashSet<u16>) -> AppResult<u16> {
    let start = if preferred == 0 { 1 } else { preferred };
    for port in start..=u16::MAX {
        if !reserved.contains(&port) {
            return Ok(port);
        }
    }
    Err(AppError::Message(format!(
        "无法从端口 {preferred} 起找到可用本地端口"
    )))
}

/// Validate an update without blocking a repair because another, unchanged
/// workspace already has a legacy duplicate resource.
pub fn validate_workspace_resources_update(
    profiles: &[WorkspaceProfile],
    current: &WorkspaceProfile,
    candidate: &WorkspaceProfile,
) -> AppResult<()> {
    if !claim_changed(current, candidate) {
        return Ok(());
    }
    validate_claim_against_others(profiles, candidate)
}

pub fn validate_service_start(profiles: &[WorkspaceProfile], workspace_id: &str) -> AppResult<()> {
    let target = profiles
        .iter()
        .find(|profile| profile.id == workspace_id)
        .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}")))?;
    validate_claim_against_others(profiles, target)
}

fn validate_claim_against_others(
    profiles: &[WorkspaceProfile],
    candidate: &WorkspaceProfile,
) -> AppResult<()> {
    let target = ResourceClaim::from_profile(candidate);
    for owner in profiles.iter().filter(|profile| profile.id != candidate.id) {
        let owner = ResourceClaim::from_profile(owner);
        if owner.local_port == target.local_port {
            return Err(port_conflict_error(target, owner));
        }
        if same_non_empty_subdomain(target, owner) {
            return Err(subdomain_conflict_error(target, owner));
        }
    }
    Ok(())
}

fn claim_changed(current: &WorkspaceProfile, next: &WorkspaceProfile) -> bool {
    let current = ResourceClaim::from_profile(current);
    let next = ResourceClaim::from_profile(next);
    current.local_port != next.local_port
        || current.subdomain != next.subdomain
        || current.uses_frp != next.uses_frp
}

fn same_non_empty_subdomain(left: ResourceClaim<'_>, right: ResourceClaim<'_>) -> bool {
    if !left.uses_frp || !right.uses_frp {
        return false;
    }
    let left = left.subdomain.trim();
    let right = right.subdomain.trim();
    !left.is_empty() && !right.is_empty() && left.eq_ignore_ascii_case(right)
}

fn port_conflict_error(target: ResourceClaim<'_>, owner: ResourceClaim<'_>) -> AppError {
    AppError::Message(format!(
        "本地端口 {} 与工作区“{}”的 MCP 服务重复。请修改当前工作区 MCP 端口后再启动。",
        target.local_port, owner.profile.name,
    ))
}

fn subdomain_conflict_error(target: ResourceClaim<'_>, owner: ResourceClaim<'_>) -> AppError {
    AppError::Message(format!(
        "FRP 子域名“{}”已被工作区“{}”的 MCP 服务使用，当前工作区 MCP 不能启动。",
        target.subdomain.trim(),
        owner.profile.name,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        assign_free_workspace_ports, validate_service_start, validate_workspace_resources,
        validate_workspace_resources_update,
    };
    use crate::workspace::WorkspaceProfile;

    fn profile(name: &str, mcp_port: u16) -> WorkspaceProfile {
        let mut profile = WorkspaceProfile::new(format!("C:/workspace/{name}"), Some(name.into()));
        profile.runtime.local_port = mcp_port;
        profile.tunnel.tunnel_type = "frp".into();
        profile.tunnel.frp_subdomain = format!("{name}-mcp");
        profile
    }

    #[test]
    fn rejects_duplicate_mcp_port_across_workspaces_before_start() {
        let owner = profile("owner", 28_766);
        let target = profile("target", 28_766);

        let error =
            validate_service_start(&[owner.clone(), target.clone()], &target.id).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("28766"));
        assert!(message.contains(&owner.name));
        assert!(message.contains("MCP"));
    }

    #[test]
    fn assign_free_ports_keeps_defaults_when_available() {
        let mut candidate = WorkspaceProfile::new("C:/workspace/new".into(), Some("new".into()));
        assign_free_workspace_ports(&[], &mut candidate).expect("assign");
        assert_eq!(candidate.runtime.local_port, 28_766);
    }

    #[test]
    fn assign_free_ports_skips_ports_claimed_by_other_workspaces() {
        let owner = profile("owner", 28_766);
        let mut candidate = WorkspaceProfile::new("C:/workspace/new".into(), Some("new".into()));
        assign_free_workspace_ports(std::slice::from_ref(&owner), &mut candidate).expect("assign");
        assert_eq!(candidate.runtime.local_port, 28_767);
        assert!(validate_workspace_resources(&[owner], &candidate).is_ok());
    }

    #[test]
    fn allows_a_workspace_to_keep_its_own_ports() {
        let original = profile("target", 28_766);
        let updated = original.clone();
        assert!(validate_workspace_resources(&[original], &updated).is_ok());
    }

    #[test]
    fn allows_fixing_mcp_port_when_other_fields_are_unchanged() {
        let owner = profile("owner", 28_765);
        let current = profile("target", 28_766);
        let mut candidate = current.clone();
        candidate.runtime.local_port = 28_767;
        assert!(validate_workspace_resources_update(
            &[owner, current.clone()],
            &current,
            &candidate,
        )
        .is_ok());
    }

    #[test]
    fn rejects_changed_mcp_port_that_conflicts_with_another_workspace() {
        let owner = profile("owner", 28_765);
        let current = profile("target", 28_766);
        let mut candidate = current.clone();
        candidate.runtime.local_port = owner.runtime.local_port;
        let error =
            validate_workspace_resources_update(&[owner, current.clone()], &current, &candidate)
                .unwrap_err();
        assert!(error.to_string().contains("28765"));
    }

    #[test]
    fn unrelated_update_is_not_blocked_by_legacy_duplicates() {
        let first = profile("first", 28_766);
        let second = profile("second", 28_766);
        let candidate = profile("candidate", 28_769);
        assert!(validate_workspace_resources(&[first, second], &candidate).is_ok());
    }

    #[test]
    fn start_is_blocked_when_target_participates_in_legacy_duplicate() {
        let first = profile("first", 28_766);
        let target = profile("target", 28_766);
        let error = validate_service_start(&[first, target.clone()], &target.id).unwrap_err();
        assert!(error.to_string().contains("端口"));
    }

    #[test]
    fn rejects_duplicate_subdomains_with_owner_details() {
        let owner = profile("owner", 28_765);
        let mut candidate = profile("target", 28_766);
        candidate.tunnel.frp_subdomain = owner.tunnel.frp_subdomain.clone();
        let error =
            validate_workspace_resources(std::slice::from_ref(&owner), &candidate).unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&owner.tunnel.frp_subdomain));
        assert!(message.contains(&owner.name));
        assert!(message.contains("MCP"));
    }

    #[test]
    fn ignores_blank_subdomain_claims() {
        let mut first = profile("first", 28_765);
        let mut second = profile("second", 28_766);
        first.tunnel.frp_subdomain.clear();
        second.tunnel.frp_subdomain.clear();
        assert!(validate_workspace_resources(&[first], &second).is_ok());
    }
}
