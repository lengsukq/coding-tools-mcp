import { invoke } from "@tauri-apps/api/core";

export interface GlobalGatewayConfigDto {
  enabled: boolean;
  localPort: number;
  tunnelType: string;
  publicUrl: string;
  cloudflareMode: string;
  frpProfileId: string;
  frpServer: string;
  frpSubdomain: string;
  frpServerPort: number;
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

export const DEFAULT_GLOBAL_GATEWAY: GlobalGatewayConfigDto = {
  enabled: false,
  localPort: 28765,
  tunnelType: "none",
  publicUrl: "",
  cloudflareMode: "quick",
  frpProfileId: "",
  frpServer: "",
  frpSubdomain: "",
  frpServerPort: 7000,
  useProxy: true,
};

export const getGlobalGatewayConfig = () => invoke<GlobalGatewayConfigDto>("get_global_gateway_config");
export const setGlobalGatewayConfig = (config: GlobalGatewayConfigDto) => invoke<void>("set_global_gateway_config", { config });
export const startGlobalGateway = () => invoke<GlobalGatewayStatusDto>("start_global_gateway");
export const stopGlobalGateway = () => invoke<void>("stop_global_gateway");
export const getGlobalGatewayStatus = () => invoke<GlobalGatewayStatusDto>("get_global_gateway_status");
export const checkGlobalGatewayHealth = () => invoke<GatewayHealthItemDto[]>("check_global_gateway_health");
