/** Gemini Apps only accepts a publicly reachable HTTPS Streamable HTTP MCP URL. */
export const GEMINI_APP_URL = "https://gemini.google.com/";
export const GEMINI_DEVELOPER_MODE_URL = "https://gemini.google.com/devmode";

export interface GeminiConnectionInput {
  endpoint: string;
  publicEnabled: boolean;
  mcpRunning: boolean;
  authType: string | null;
  publicHealth: "passed" | "failed" | "unchecked";
  configDirty: boolean;
}

function isPrivateIpv4(hostname: string): boolean {
  const parts = hostname.split(".").map(Number);
  if (parts.length !== 4 || parts.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return false;
  return parts[0] === 10 || parts[0] === 127 || parts[0] === 0 ||
    (parts[0] === 192 && parts[1] === 168) ||
    (parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31) ||
    (parts[0] === 169 && parts[1] === 254);
}

export function validGeminiMcpEndpoint(input: string): boolean {
  try {
    const url = new URL(input.trim());
    const hostname = url.hostname.toLowerCase();
    return url.protocol === "https:" &&
      url.pathname === "/mcp" && !url.search && !url.hash &&
      !url.username && !url.password &&
      hostname !== "localhost" && !hostname.endsWith(".local") &&
      hostname !== "[::1]" && hostname !== "::1" &&
      !isPrivateIpv4(hostname);
  } catch {
    return false;
  }
}

export function assessGeminiConnection(input: GeminiConnectionInput) {
  const validEndpoint = validGeminiMcpEndpoint(input.endpoint);
  const issues: string[] = [];
  if (!input.publicEnabled) issues.push("请先启用公网连接，并配置 Cloudflare、FRP 或自己的 HTTPS 反向代理。");
  if (!validEndpoint) issues.push("需要公网可访问的 HTTPS /mcp 地址，不能使用本地或内网地址。");
  if (!input.mcpRunning) issues.push("请先启动 Global MCP 服务。");
  if (input.authType !== "oauth") issues.push("请在应用设置 → 连接 → 认证与访问范围中选择 OAuth，然后重启 MCP 服务。Gemini 自定义应用优先使用 OAuth/DCR。");
  if (input.configDirty) issues.push("公网配置尚未保存，请先保存并应用配置后再复制地址。");
  if (input.publicHealth === "failed") issues.push("公网健康检查失败，请修复后再尝试连接 Gemini。");
  if (input.publicHealth === "unchecked") issues.push("尚未验证公网端点，建议先点击连接健康中的健康检查。");
  return {
    validEndpoint,
    canCopy: input.publicEnabled && validEndpoint && !input.configDirty,
    ready: input.publicEnabled && validEndpoint && input.mcpRunning && input.authType === "oauth" &&
      !input.configDirty && input.publicHealth === "passed",
    issues,
  };
}
