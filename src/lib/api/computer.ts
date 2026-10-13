import { invoke } from "@tauri-apps/api/core";

export interface ComputerScreenStatus {
  pendingRequestId: string | null;
  pendingExpiresInSeconds: number | null;
  active: boolean;
  paused: boolean;
  expiresInSeconds: number | null;
  supported: boolean;
  screenRecordingGranted: boolean | null;
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

export const requestComputerScreenPermission = () =>
  invoke<ComputerScreenStatus>("request_computer_screen_permission");

export const requestComputerAccessibilityPermission = () =>
  invoke<ComputerScreenStatus>("request_computer_accessibility_permission");
