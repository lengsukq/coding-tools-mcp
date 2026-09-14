import { invoke } from "@tauri-apps/api/core";

export interface WebviewMemorySample {
  mainMb: number;
  webviewMb: number;
  webviewProcessCount: number;
  supported: boolean;
}

/** Sample UI process memory. Does not touch MCP / FRP. */
export async function getWebviewMemorySample(): Promise<WebviewMemorySample> {
  return invoke<WebviewMemorySample>("get_webview_memory_sample");
}

/**
 * Destroy and recreate the main WebView (replaces msedgewebview2 processes).
 * Does not stop MCP / FRP.
 */
export async function recreateUiWebview(): Promise<void> {
  return invoke("recreate_ui_webview");
}
