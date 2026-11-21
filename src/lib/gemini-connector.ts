/** Gemini Apps only accepts a publicly reachable HTTPS Streamable HTTP MCP URL. */
export const GEMINI_APP_URL = "https://gemini.google.com/";
export const GEMINI_DEVELOPER_MODE_URL = "https://gemini.google.com/devmode";
/** Native Rust stdio-to-Streamable-HTTP bridge embedded in the desktop binary avoids slow npx cold starts and browser OAuth timeouts. */
export const GEMINI_LOCAL_DEFAULT_BIN = "~/.local/bin/coding-tools-mcp";
export const GEMINI_LOCAL_AUTH_FILE = "~/.local/share/coding-tools-mcp/gemini-stdio-auth.json";
export const GEMINI_LOCAL_SERVER_NAME = "Coding Tools MCP";

export interface GeminiLocalConnectionInput {
  localEndpoint: string;
  mcpRunning: boolean;
  authType: string | null;
  bridgeCommand?: string | null;
  binaryReady?: boolean;
  credentialsReady?: boolean;
  bridgeIssue?: string | null;
}

/** Only the running app's loopback listener may be used as a local command target. */
export function validGeminiLocalEndpoint(input: string): boolean {
  try {
    const url = new URL(input.trim());
    return url.protocol === "http:" &&
      (url.hostname === "127.0.0.1" || url.hostname === "localhost") &&
      !!url.port && url.pathname === "/mcp" && !url.search && !url.hash &&
      !url.username && !url.password;
  } catch {
    return false;
  }
}

export function geminiLocalCommand(localEndpoint: string, binaryPath?: string | null): string | null {
  if (!validGeminiLocalEndpoint(localEndpoint)) return null;
  const port = new URL(localEndpoint.trim()).port;
  const bin = binaryPath?.trim() || GEMINI_LOCAL_DEFAULT_BIN;
  const quotedBin = /\s/.test(bin) ? `"${bin}"` : bin;
  // Normalizing the endpoint prevents injecting arguments from user-editable URLs.
  return `${quotedBin} mcp-stdio http://127.0.0.1:${port}/mcp`;
}

export function assessGeminiLocalConnection(input: GeminiLocalConnectionInput) {
  const fallbackCommand = geminiLocalCommand(input.localEndpoint);
  const command = fallbackCommand ? (input.bridgeCommand?.trim() || fallbackCommand) : null;
  const authSupported = input.authType === "oauth" || input.authType === "noauth";
  const bridgePrepared = input.binaryReady !== false && input.credentialsReady !== false;
  const issues: string[] = [];
  if (!command) issues.push("本地 MCP 地址必须是 127.0.0.1 或 localhost 的 HTTP /mcp 端点（含端口）。");
  if (!input.mcpRunning) issues.push("请先启动 Global MCP 服务，本地桥接不会单独启动桌面应用。");
  if (input.authType === "bearer") issues.push("当前是 Bearer 模式；为避免在命令中暴露密钥，不自动生成带 Token 的配置。可使用 OAuth，或手动配置安全的凭据文件。");
  if (!input.authType) issues.push("尚未读取到 Global MCP 认证模式，请先刷新连接状态。");
  if (input.authType === "noauth") issues.push("当前为无认证模式，仅适用于可信本机环境；建议使用 OAuth。");
  if (authSupported && !bridgePrepared) {
    issues.push(input.bridgeIssue?.trim() || "尚未准备本地桥接入口或 OAuth 凭据，请点击“生成凭据并复制”。");
  }
  return {
    command,
    ready: !!command && input.mcpRunning && authSupported && bridgePrepared,
    canCopy: !!command && authSupported,
    needsPrepare: authSupported && !bridgePrepared,
    issues,
  };
}

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
  if (input.authType !== "oauth") issues.push("请在应用设置 → 连接 → 认证与访问范围中选择 OAuth（保存后自动热更新生效）。Gemini 自定义应用优先使用 OAuth/DCR。");
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
