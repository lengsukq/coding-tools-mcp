import { invoke } from "@tauri-apps/api/core";

export interface ComputerScreenStatus {
  pendingRequestId: string | null;
  pendingExpiresInSeconds: number | null;
  active: boolean;
  expiresInSeconds: number | null;
  supported: boolean;
  screenRecordingGranted: boolean | null;
}

export const getComputerScreenStatus = () =>
  invoke<ComputerScreenStatus>("get_computer_screen_status");

export const approveComputerScreenRequest = (requestId: string) =>
  invoke<ComputerScreenStatus>("approve_computer_screen_request", { requestId });

export const revokeComputerScreenAccess = () =>
  invoke<ComputerScreenStatus>("revoke_computer_screen_access");

export const requestComputerScreenPermission = () =>
  invoke<ComputerScreenStatus>("request_computer_screen_permission");
