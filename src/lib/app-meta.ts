import packageJson from "../../package.json";

/** Application version synchronized across desktop manifests. */
export const APP_VERSION = packageJson.version;

/** Canonical public links for Coding Tools MCP. */
export const REPO_URL = "https://github.com/lengsukq/coding-tools-mcp";
export const RELEASES_LATEST_URL = `${REPO_URL}/releases/latest`;
