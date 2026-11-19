import { invoke } from "@tauri-apps/api/core";

export interface LocalDeviceDto {
  deviceId: string;
  deviceName: string;
  fingerprint: string;
  os: string;
  arch: string;
  version: string;
  mcpPort: number;
  authType: string;
  localAgentEndpoint: string;
  addresses: string[];
  allowLanAccess: boolean;
  allowLocalNoauth: boolean;
  lanEnabled: boolean;
  shareMode: "all" | "selected" | string;
  sharedWorkspaceIds: string[];
  trustLevel: "full" | "readonly" | string;
  allowComputerUse: boolean;
  extraSubnets: string[];
  computerUseSupported: boolean;
  computerUseEnabled: boolean;
  workspaceCount: number;
}

export interface PairedPeerConfigDto {
  deviceId: string;
  deviceName: string;
  fingerprint: string;
  os: string;
  arch: string;
  version: string;
  host: string;
  port: number;
  addresses: string[];
  enabled: boolean;
  pairedAtMs: number;
}

export interface LanFederationConfigDto {
  enabled: boolean;
  deviceId: string;
  deviceName: string;
  shareMode: "all" | "selected" | string;
  sharedWorkspaceIds: string[];
  trustLevel: "full" | "readonly" | string;
  allowComputerUse: boolean;
  extraSubnets: string[];
  pairedPeers: PairedPeerConfigDto[];
}

export interface PairingPinDto {
  pin: string;
  createdAtMs: number;
  expiresAtMs: number;
  remainingSeconds: number;
  remainingAttempts: number;
}

export interface DiscoveredPeerDto {
  deviceId: string;
  deviceName: string;
  fingerprint: string;
  os: string;
  arch: string;
  version: string;
  host: string;
  port: number;
  addresses: string[];
  computerUseSupported: boolean;
  alreadyPaired: boolean;
  lastSeenMs: number;
}

export interface PairedPeerStatusDto {
  deviceId: string;
  deviceName: string;
  fingerprint: string;
  os: string;
  arch: string;
  version: string;
  host: string;
  port: number;
  addresses: string[];
  enabled: boolean;
  pairedAtMs: number;
  online: boolean;
  latencyMs: number | null;
  trustLevel: string;
  workspaceCount: number;
  computerUseSupported: boolean;
  computerUseEnabled: boolean;
  lastError: string | null;
}

export interface LanOverviewDto {
  localDevice: LocalDeviceDto;
  config: LanFederationConfigDto;
  activePin: PairingPinDto | null;
  discoveredPeers: DiscoveredPeerDto[];
  pairedPeers: PairedPeerStatusDto[];
  mcpRunning: boolean;
}

export interface LanSettingsUpdateDto {
  enabled: boolean;
  deviceName: string;
  shareMode: string;
  sharedWorkspaceIds: string[];
  trustLevel: string;
  allowComputerUse: boolean;
  extraSubnets: string[];
  allowLocalNoauth?: boolean;
}

export interface LanSettingsSaveResultDto {
  overview: LanOverviewDto;
  listenerRestarted: boolean;
}

export function getLanPeersOverview(forceRefresh = false): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("get_lan_peers_overview", { forceRefresh });
}

export function saveLanFederationSettings(input: LanSettingsUpdateDto): Promise<LanSettingsSaveResultDto> {
  return invoke<LanSettingsSaveResultDto>("save_lan_federation_settings", { input });
}

export function createLanPairingPin(): Promise<PairingPinDto> {
  return invoke<PairingPinDto>("create_lan_pairing_pin");
}

export function cancelLanPairingPin(): Promise<void> {
  return invoke<void>("cancel_lan_pairing_pin");
}

export function scanLanPeers(): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("scan_lan_peers");
}

export function probeLanPeerAddress(host: string, port: number): Promise<DiscoveredPeerDto> {
  return invoke<DiscoveredPeerDto>("probe_lan_peer_address", { host, port });
}

export function pairLanPeer(host: string, port: number, pin: string): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("pair_lan_peer", { host, port, pin });
}

export function removeLanPairedPeer(deviceId: string): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("remove_lan_paired_peer", { deviceId });
}

export function setLanPairedPeerEnabled(deviceId: string, enabled: boolean): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("set_lan_paired_peer_enabled", { deviceId, enabled });
}

export function renameLanPairedPeer(deviceId: string, deviceName: string): Promise<LanOverviewDto> {
  return invoke<LanOverviewDto>("rename_lan_paired_peer", { deviceId, deviceName });
}
