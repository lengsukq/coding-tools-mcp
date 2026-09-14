import { invoke } from "@tauri-apps/api/core";

export interface TunnelStatus {
  state: string;
  publicUrl: string;
  tunnelPid: number | null;
}

export async function stopTunnel(id: string): Promise<TunnelStatus> {
  return invoke<TunnelStatus>("stop_tunnel", { id });
}

export interface TunnelTestResult {
  success: boolean;
  publicUrl: string;
  keptRunning: boolean;
  message: string;
}

export async function testTunnel(id: string): Promise<TunnelTestResult> {
  return invoke<TunnelTestResult>("test_tunnel", { id });
}

export async function restartTunnel(id: string): Promise<TunnelStatus> {
  return invoke<TunnelStatus>("restart_tunnel", { id });
}
