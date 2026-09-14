import { invoke } from "@tauri-apps/api/core";

export function hideToTray(): Promise<void> {
  return invoke("hide_to_tray");
}

export function quitApp(): Promise<void> {
  return invoke("quit_app");
}
