import { invoke } from "@tauri-apps/api/core";

export function hideToTray(): Promise<void> {
  return invoke("hide_to_tray");
}

/** macOS: destroy WKWebView, retain MCP/backend and menubar icon. */
export function enterCompactMode(): Promise<void> {
  return invoke("enter_compact_mode");
}

export function quitApp(): Promise<void> {
  return invoke("quit_app");
}
