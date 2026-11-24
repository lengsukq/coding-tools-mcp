import { invoke } from "@tauri-apps/api/core";

export interface GlobalGatewayConfigDto {
  enabled: boolean;
  localPort: number;
  tunnelType: string;
  publicUrl: string;
  cloudflareMode: string;
  cloudflareHttp2: boolean;
  frpProfileId: string;
  frpServer: string;
  frpSubdomain: string;
  frpServerPort: number;
  frpTls: boolean;
  useProxy: boolean;
}

export interface GlobalGatewayStatusDto {
  state: string;
  localUrl: string;
  publicUrl: string;
  detail: string;
}

export interface GatewayHealthItemDto {
  label: string;
  ok: boolean;
  detail: string;
}

export interface GeminiLocalBridgeStatusDto {
  command: string;
  binaryPath: string;
  binaryReady: boolean;
  authFilePath: string;
  credentialsReady: boolean;
  authType: string;
  endpoint: string;
  issue: string;
}

export const DEFAULT_GLOBAL_GATEWAY: GlobalGatewayConfigDto = {
  enabled: false,
  localPort: 28765,
  tunnelType: "none",
  publicUrl: "",
  cloudflareMode: "quick",
  cloudflareHttp2: true,
  frpProfileId: "",
  frpServer: "",
  frpSubdomain: "",
  frpServerPort: 7000,
  frpTls: false,
  useProxy: true,
};

export const getGlobalGatewayConfig = () => invoke<GlobalGatewayConfigDto>("get_global_gateway_config");
export const setGlobalGatewayConfig = (config: GlobalGatewayConfigDto) => invoke<void>("set_global_gateway_config", { config });
export const restartGlobalCloudflareTunnel = () => invoke<GlobalGatewayStatusDto>("restart_global_cloudflare_tunnel");
export const getGlobalGatewayStatus = () => invoke<GlobalGatewayStatusDto>("get_global_gateway_status");
export const checkGlobalGatewayHealth = () => invoke<GatewayHealthItemDto[]>("check_global_gateway_health");
export const getGeminiLocalBridgeStatus = () => invoke<GeminiLocalBridgeStatusDto>("get_gemini_local_bridge_status");
export const prepareGeminiLocalBridge = () => invoke<GeminiLocalBridgeStatusDto>("prepare_gemini_local_bridge");
