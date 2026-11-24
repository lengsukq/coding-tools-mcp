import type {
  UpstreamExposeMode,
  UpstreamSecretPatch,
  UpstreamServerConfig,
} from "$lib/api/upstream-mcp";

export interface KeyValueDraftRow {
  key: string;
  value: string;
  secret: boolean;
  /** True when backed by an existing stored secret whose plaintext is hidden until overwritten. */
  hasStoredSecret?: boolean;
}

export interface UpstreamServerDraft {
  id: string;
  alias: string;
  name: string;
  description: string;
  enabled: boolean;
  exposeMode: UpstreamExposeMode;
  transportType: "stdio" | "streamable_http" | "sse";
  command: string;
  argsText: string;
  cwd: string;
  url: string;
  useProxy: boolean;
  hasStoredBearer: boolean;
  bearerTokenInput: string;
  clearBearerToken: boolean;
  envRows: KeyValueDraftRow[];
  headerRows: KeyValueDraftRow[];
  callTimeoutSecs: string;
  idleTimeoutSecs: string;
  source: string;
  disabledTools: string[];
  originalSecretEnvKeys: string[];
  originalSecretHeaderKeys: string[];
}

export interface ImportCandidateSelectionDraft {
  selected: boolean;
  alias: string;
  exposeMode: UpstreamExposeMode;
}

export function createEmptyUpstreamDraft(): UpstreamServerDraft {
  return {
    id: "",
    alias: "",
    name: "",
    description: "",
    enabled: true,
    exposeMode: "direct",
    transportType: "stdio",
    command: "",
    argsText: "",
    cwd: "",
    url: "",
    useProxy: false,
    hasStoredBearer: false,
    bearerTokenInput: "",
    clearBearerToken: false,
    envRows: [],
    headerRows: [],
    callTimeoutSecs: "60",
    idleTimeoutSecs: "900",
    source: "manual",
    disabledTools: [],
    originalSecretEnvKeys: [],
    originalSecretHeaderKeys: [],
  };
}

export function buildUpstreamDraftFromConfig(cfg: UpstreamServerConfig): UpstreamServerDraft {
  const base = createEmptyUpstreamDraft();
  base.id = cfg.id;
  base.alias = cfg.alias;
  base.name = cfg.name;
  base.description = cfg.description;
  base.enabled = cfg.enabled;
  base.exposeMode = cfg.exposeMode;
  base.callTimeoutSecs = String(cfg.callTimeoutSecs || 60);
  base.idleTimeoutSecs = String(cfg.idleTimeoutSecs ?? 900);
  base.source = cfg.source || "manual";
  base.disabledTools = [...cfg.disabledTools];

  if (cfg.transport.type === "stdio") {
    base.transportType = "stdio";
    base.command = cfg.transport.command;
    base.argsText = cfg.transport.args.join("\n");
    base.cwd = cfg.transport.cwd ?? "";
    base.originalSecretEnvKeys = [...cfg.transport.secretEnvKeys];
    base.envRows = [
      ...Object.entries(cfg.transport.env).map(([key, value]) => ({
        key,
        value,
        secret: false,
        hasStoredSecret: false,
      })),
      ...cfg.transport.secretEnvKeys.map((key) => ({
        key,
        value: "",
        secret: true,
        hasStoredSecret: true,
      })),
    ];
  } else {
    base.transportType = cfg.transport.type;
    base.url = cfg.transport.url;
    base.useProxy = cfg.transport.useProxy;
    base.hasStoredBearer = cfg.transport.hasBearerToken;
    base.originalSecretHeaderKeys = [...cfg.transport.secretHeaderKeys];
    base.headerRows = [
      ...Object.entries(cfg.transport.headers).map(([key, value]) => ({
        key,
        value,
        secret: false,
        hasStoredSecret: false,
      })),
      ...cfg.transport.secretHeaderKeys.map((key) => ({
        key,
        value: "",
        secret: true,
        hasStoredSecret: true,
      })),
    ];
  }
  return base;
}

export function splitArgsText(raw: string): string[] {
  return raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

export function buildSavePayloadFromDraft(draft: UpstreamServerDraft): {
  server: UpstreamServerConfig;
  secretPatch: UpstreamSecretPatch;
} {
  const callTimeout = Math.max(5, Math.min(600, Number.parseInt(draft.callTimeoutSecs, 10) || 60));
  const parsedIdle = Number.parseInt(draft.idleTimeoutSecs, 10);
  const idleTimeout = Number.isNaN(parsedIdle) ? 900 : Math.max(0, Math.min(86400, parsedIdle));

  const secretPatch: UpstreamSecretPatch = {
    setEnvSecrets: {},
    removeEnvSecrets: [],
    setHeaderSecrets: {},
    removeHeaderSecrets: [],
  };

  if (draft.transportType === "stdio") {
    const env: Record<string, string> = {};
    const keptSecretKeys = new Set<string>();
    for (const row of draft.envRows) {
      const k = row.key.trim();
      if (!k) continue;
      if (row.secret) {
        keptSecretKeys.add(k);
        if (row.value !== "") {
          secretPatch.setEnvSecrets![k] = row.value;
        }
      } else {
        env[k] = row.value;
      }
    }
    for (const oldKey of draft.originalSecretEnvKeys) {
      if (!keptSecretKeys.has(oldKey)) {
        secretPatch.removeEnvSecrets!.push(oldKey);
      }
    }
    return {
      server: {
        id: draft.id,
        alias: draft.alias.trim(),
        name: draft.name.trim() || draft.alias.trim(),
        description: draft.description.trim(),
        enabled: draft.enabled,
        exposeMode: draft.exposeMode,
        transport: {
          type: "stdio",
          command: draft.command.trim(),
          args: splitArgsText(draft.argsText),
          cwd: draft.cwd.trim() || null,
          env,
          secretEnvKeys: [...keptSecretKeys],
        },
        disabledTools: draft.disabledTools,
        callTimeoutSecs: callTimeout,
        idleTimeoutSecs: idleTimeout,
        source: draft.source || "manual",
      },
      secretPatch,
    };
  }

  const headers: Record<string, string> = {};
  const keptSecretHeaders = new Set<string>();
  for (const row of draft.headerRows) {
    const k = row.key.trim();
    if (!k) continue;
    if (row.secret) {
      keptSecretHeaders.add(k);
      if (row.value !== "") {
        secretPatch.setHeaderSecrets![k] = row.value;
      }
    } else {
      headers[k] = row.value.trim();
    }
  }
  for (const oldKey of draft.originalSecretHeaderKeys) {
    if (!keptSecretHeaders.has(oldKey)) {
      secretPatch.removeHeaderSecrets!.push(oldKey);
    }
  }
  if (draft.clearBearerToken) {
    secretPatch.bearerToken = null;
  } else if (draft.bearerTokenInput.trim()) {
    secretPatch.bearerToken = draft.bearerTokenInput.trim();
  }

  return {
    server: {
      id: draft.id,
      alias: draft.alias.trim(),
      name: draft.name.trim() || draft.alias.trim(),
      description: draft.description.trim(),
      enabled: draft.enabled,
      exposeMode: draft.exposeMode,
      transport: {
        type: draft.transportType,
        url: draft.url.trim(),
        headers,
        secretHeaderKeys: [...keptSecretHeaders],
        hasBearerToken:
          !draft.clearBearerToken &&
          (draft.hasStoredBearer || Boolean(draft.bearerTokenInput.trim())),
        useProxy: draft.useProxy,
      },
      disabledTools: draft.disabledTools,
      callTimeoutSecs: callTimeout,
      idleTimeoutSecs: idleTimeout,
      source: draft.source || "manual",
    },
    secretPatch,
  };
}

export function transportLabel(kind: string): string {
  switch (kind) {
    case "stdio":
      return "Stdio";
    case "streamable_http":
      return "Streamable HTTP";
    case "sse":
      return "Legacy SSE";
    default:
      return kind;
  }
}

export function upstreamStatusForPill(status: string): { status: string; label: string } {
  switch (status) {
    case "connected":
      return { status: "running", label: "已连接" };
    case "connecting":
      return { status: "starting", label: "连接中" };
    case "error":
      return { status: "error", label: "异常" };
    case "disabled":
      return { status: "stopped", label: "已停用" };
    default:
      return { status: "idle", label: "待命（懒启动）" };
  }
}
