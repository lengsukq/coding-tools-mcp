import { invoke } from "@tauri-apps/api/core";

/** Identity of the current desktop process, not macOS Settings' app label. */
export interface ComputerScreenPermissionDiagnostic {
  ok: boolean;
  bundle_id?: string;
  expected_bundle_id?: string;
  bundle_path?: string;
  executable_path?: string;
  process_name?: string;
  process_id?: number;
  is_app_bundle?: boolean;
  preflight_granted?: boolean;
  signature_type?: "ad_hoc" | "certificate" | "unsigned_or_unknown" | "unavailable";
  signing_team_id?: string;
  signing_identifier?: string;
  signature_diagnostic_error?: number;
  error?: string;
}

export interface ComputerScreenStatus {
  pendingRequestId: string | null;
  pendingExpiresInSeconds: number | null;
  active: boolean;
  paused: boolean;
  pendingSensitiveActionId: string | null;
  pendingSensitiveActionDescription: string | null;
  pendingSensitiveActionApp: string | null;
  pendingSensitiveActionApproved: boolean;
  pendingSensitiveActionExpiresInSeconds: number | null;
  expiresInSeconds: number | null;
  supported: boolean;
  screenRecordingGranted: boolean | null;
  screenCaptureSucceededInSession: boolean;
  screenPermissionDiagnostic: ComputerScreenPermissionDiagnostic;
  accessibilityGranted: boolean | null;
  pendingScope: "observe" | "control" | null;
  activeScope: "observe" | "control" | null;
  targetAppBundle: string | null;
  remainingControlActions: number | null;
}

export const getComputerScreenStatus = () =>
  invoke<ComputerScreenStatus>("get_computer_screen_status");

export const approveComputerScreenRequest = (requestId: string, targetBundle?: string) =>
  invoke<ComputerScreenStatus>("approve_computer_screen_request", { requestId, targetBundle: targetBundle || null });

export interface ComputerApplication {
  bundle_id: string;
  name: string;
}

export const listComputerAvailableApps = () =>
  invoke<{ ok: boolean; applications: ComputerApplication[] }>("list_computer_available_apps");

export const revokeComputerScreenAccess = () =>
  invoke<ComputerScreenStatus>("revoke_computer_screen_access");

export const pauseComputerAccess = () =>
  invoke<ComputerScreenStatus>("pause_computer_access");

export const resumeComputerAccess = () =>
  invoke<ComputerScreenStatus>("resume_computer_access");

export const decideComputerSensitiveAction = (confirmationId: string, approve: boolean) =>
  invoke<ComputerScreenStatus>("decide_computer_sensitive_action", { confirmationId, approve });

export const requestComputerScreenPermission = () =>
  invoke<ComputerScreenStatus>("request_computer_screen_permission");

export const requestComputerAccessibilityPermission = () =>
  invoke<ComputerScreenStatus>("request_computer_accessibility_permission");
