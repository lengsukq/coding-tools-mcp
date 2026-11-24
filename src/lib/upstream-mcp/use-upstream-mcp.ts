import { computed, onMounted, ref, watch } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  deleteUpstreamMcpServer,
  getUpstreamMcpOverview,
  importUpstreamMcpCandidates,
  listUpstreamMcpTakeoverBackups,
  parseUpstreamMcpJson,
  previewUpstreamMcpTakeover,
  restoreUpstreamMcpTakeoverBackup,
  saveUpstreamMcpServer,
  scanLocalUpstreamMcpClients,
  setUpstreamMcpGlobalSettings,
  setUpstreamMcpServerEnabled,
  setUpstreamMcpServerMode,
  syncAllUpstreamMcpServers,
  testAndSyncUpstreamMcpServer,
  updateUpstreamMcpDisabledTools,
  type HarnessTakeoverBackupRecord,
  type HarnessTakeoverPlanItem,
  type ImportedUpstreamCandidate,
  type ImportSelectionItem,
  type TakeoverConnectMode,
  type UpstreamExposeMode,
  type UpstreamMcpOverviewDto,
  type UpstreamSecretPatch,
  type UpstreamServerConfig,
  type UpstreamServerOverviewDto,
} from "$lib/api/upstream-mcp";
import { showToast } from "$lib/stores/toast";

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

export function useUpstreamMcp() {
  const loading = ref(true);
  const loadError = ref("");
  const overview = ref<UpstreamMcpOverviewDto | null>(null);
  const globalBusy = ref(false);
  const syncingAll = ref(false);
  const busyServerId = ref<string | null>(null);
  const testingServerId = ref<string | null>(null);

  // Editor modal
  const editorOpen = ref(false);
  const editorSaving = ref(false);
  const editorError = ref("");
  const editorDraft = ref<UpstreamServerDraft>(createEmptyUpstreamDraft());

  // Tools modal
  const toolsModalServerId = ref<string | null>(null);
  const toolsSaving = ref(false);
  const toolsModalServer = computed<UpstreamServerOverviewDto | null>(() => {
    if (!toolsModalServerId.value || !overview.value) return null;
    return overview.value.servers.find((s) => s.config.id === toolsModalServerId.value) ?? null;
  });

  // Import modal
  const importOpen = ref(false);
  const importTab = ref<"scan" | "json">("scan");
  const scanningClients = ref(false);
  const parsingJson = ref(false);
  const importingBusy = ref(false);
  const importError = ref("");
  const importJsonText = ref("");
  const scannedCandidates = ref<ImportedUpstreamCandidate[]>([]);
  const parsedCandidates = ref<ImportedUpstreamCandidate[]>([]);
  const candidateSelections = ref<Record<string, ImportCandidateSelectionDraft>>({});

  // Aggregate & Replace (Harness takeover) state
  const replaceSourceConfigs = ref(false);
  const takeoverConnectMode = ref<TakeoverConnectMode>("auto");
  const takeoverPreview = ref<HarnessTakeoverPlanItem[]>([]);
  const loadingTakeoverPreview = ref(false);
  const takeoverBackups = ref<HarnessTakeoverBackupRecord[]>([]);
  const restoringBackupId = ref<string | null>(null);

  const activeImportCandidates = computed(() =>
    importTab.value === "scan" ? scannedCandidates.value : parsedCandidates.value,
  );

  function buildCurrentImportSelections(): ImportSelectionItem[] {
    const out: ImportSelectionItem[] = [];
    for (const item of activeImportCandidates.value) {
      if (item.blockedReason) continue;
      const sel = candidateSelections.value[item.candidateId];
      if (!sel?.selected) continue;
      out.push({
        candidateId: item.candidateId,
        aliasOverride: sel.alias.trim() || item.suggestedAlias,
        exposeMode: sel.exposeMode,
        enabled: true,
      });
    }
    return out;
  }

  let previewSeq = 0;
  async function refreshTakeoverPreview() {
    if (!importOpen.value || importTab.value !== "scan" || !replaceSourceConfigs.value) {
      takeoverPreview.value = [];
      return;
    }
    const selections = buildCurrentImportSelections();
    if (!selections.length) {
      takeoverPreview.value = [];
      return;
    }
    const seq = ++previewSeq;
    loadingTakeoverPreview.value = true;
    try {
      const plans = await previewUpstreamMcpTakeover(selections, takeoverConnectMode.value);
      if (seq === previewSeq) {
        takeoverPreview.value = plans;
      }
    } catch (error) {
      if (seq === previewSeq) {
        takeoverPreview.value = [];
        importError.value = String(error);
      }
    } finally {
      if (seq === previewSeq) {
        loadingTakeoverPreview.value = false;
      }
    }
  }

  watch(
    () => [
      importOpen.value,
      importTab.value,
      replaceSourceConfigs.value,
      takeoverConnectMode.value,
      JSON.stringify(candidateSelections.value),
    ],
    () => {
      void refreshTakeoverPreview();
    },
  );

  const directToolsOverLimit = computed(() => {
    if (!overview.value) return false;
    return overview.value.directExposedToolCount > overview.value.directToolsWarningThreshold;
  });

  function initSelections(items: ImportedUpstreamCandidate[]) {
    const next: Record<string, ImportCandidateSelectionDraft> = { ...candidateSelections.value };
    for (const item of items) {
      next[item.candidateId] = {
        selected: !item.blockedReason && !item.alreadyExistsAlias,
        alias: item.suggestedAlias,
        exposeMode: "direct",
      };
    }
    candidateSelections.value = next;
  }

  async function refresh() {
    loading.value = true;
    loadError.value = "";
    try {
      const [ov, backups] = await Promise.all([
        getUpstreamMcpOverview(),
        listUpstreamMcpTakeoverBackups().catch(() => [] as HarnessTakeoverBackupRecord[]),
      ]);
      overview.value = ov;
      takeoverBackups.value = backups;
    } catch (error) {
      loadError.value = String(error);
    } finally {
      loading.value = false;
    }
  }

  async function restoreBackup(record: HarnessTakeoverBackupRecord) {
    const confirmed = await ask(
      `确定将 ${record.sourceLabel}（${record.displayPath}）还原到接管前的原始配置文件吗？这会覆盖当前该文件中的更改。`,
      { title: "还原客户端原配置", kind: "warning" },
    );
    if (!confirmed) return;
    restoringBackupId.value = record.id;
    try {
      await restoreUpstreamMcpTakeoverBackup(record.id);
      takeoverBackups.value = await listUpstreamMcpTakeoverBackups();
      showToast(`已将 ${record.sourceLabel} 配置完整还原至接管前版本`, { kind: "success" });
    } catch (error) {
      showToast(`还原失败：${String(error)}`, { kind: "error" });
    } finally {
      restoringBackupId.value = null;
    }
  }

  async function updateGlobalSettings(enabled: boolean, alwaysExposeProxyTools: boolean) {
    globalBusy.value = true;
    try {
      overview.value = await setUpstreamMcpGlobalSettings(enabled, alwaysExposeProxyTools);
      showToast("已更新 MCP 聚合全局设置", { kind: "success" });
    } catch (error) {
      showToast(`更新失败：${String(error)}`, { kind: "error" });
    } finally {
      globalBusy.value = false;
    }
  }

  function openCreateModal() {
    editorError.value = "";
    editorDraft.value = createEmptyUpstreamDraft();
    editorOpen.value = true;
  }

  function openEditModal(item: UpstreamServerOverviewDto) {
    editorError.value = "";
    editorDraft.value = buildUpstreamDraftFromConfig(item.config);
    editorOpen.value = true;
  }

  async function submitEditor(draft: UpstreamServerDraft) {
    editorSaving.value = true;
    editorError.value = "";
    try {
      const { server, secretPatch } = buildSavePayloadFromDraft(draft);
      const isNew = !server.id;
      overview.value = await saveUpstreamMcpServer(server, secretPatch);
      editorOpen.value = false;
      showToast(
        isNew
          ? `已添加上游 MCP「${server.name}」，建议点击“测试同步”拉取工具清单`
          : `已保存上游 MCP「${server.name}」`,
        { kind: "success" },
      );
    } catch (error) {
      editorError.value = String(error);
    } finally {
      editorSaving.value = false;
    }
  }

  async function toggleServerEnabled(item: UpstreamServerOverviewDto, enabled: boolean) {
    busyServerId.value = item.config.id;
    try {
      overview.value = await setUpstreamMcpServerEnabled(item.config.id, enabled);
      showToast(
        enabled ? `已启用「${item.config.name}」` : `已停用「${item.config.name}」`,
        { kind: "success" },
      );
    } catch (error) {
      showToast(`切换失败：${String(error)}`, { kind: "error" });
    } finally {
      busyServerId.value = null;
    }
  }

  async function switchServerMode(item: UpstreamServerOverviewDto, mode: UpstreamExposeMode) {
    if (item.config.exposeMode === mode) return;
    busyServerId.value = item.config.id;
    try {
      overview.value = await setUpstreamMcpServerMode(item.config.id, mode);
      showToast(
        mode === "proxy"
          ? `「${item.config.name}」已切为懒加载代理模式（立即生效，不占 tools/list 配额）`
          : `「${item.config.name}」已切为直出模式（部分客户端需刷新 MCP 列表生效）`,
        { kind: "success" },
      );
    } catch (error) {
      showToast(`切换暴露模式失败：${String(error)}`, { kind: "error" });
    } finally {
      busyServerId.value = null;
    }
  }

  async function testAndSyncServer(item: UpstreamServerOverviewDto) {
    testingServerId.value = item.config.id;
    try {
      const res = await testAndSyncUpstreamMcpServer(item.config.id);
      overview.value = await getUpstreamMcpOverview();
      if (res.ok) {
        showToast(
          `「${item.config.name}」连通成功 · ${res.toolCount} 个工具 (${res.latencyMs} ms)`,
          { kind: "success" },
        );
      } else {
        showToast(`「${item.config.name}」连接失败：${res.error ?? "未知错误"}`, { kind: "error" });
      }
    } catch (error) {
      showToast(`测试失败：${String(error)}`, { kind: "error" });
    } finally {
      testingServerId.value = null;
    }
  }

  async function syncAllEnabled() {
    syncingAll.value = true;
    try {
      const results = await syncAllUpstreamMcpServers();
      overview.value = await getUpstreamMcpOverview();
      const okCount = results.filter((r) => r.ok).length;
      const failCount = results.length - okCount;
      if (results.length === 0) {
        showToast("当前没有已启用的上游 MCP 需要同步", { kind: "info" });
      } else if (failCount === 0) {
        showToast(`已同步全部 ${okCount} 个启用上游 MCP`, { kind: "success" });
      } else {
        showToast(`同步完成：${okCount} 个成功，${failCount} 个失败`, { kind: "warning" });
      }
    } catch (error) {
      showToast(`批量同步失败：${String(error)}`, { kind: "error" });
    } finally {
      syncingAll.value = false;
    }
  }

  async function removeServer(item: UpstreamServerOverviewDto) {
    const confirmed = await ask(
      `确定要移除上游 MCP「${item.config.name} (${item.config.alias})」吗？关联的加密凭据和工具缓存也会一并清理。`,
      { title: "移除上游 MCP", kind: "warning" },
    );
    if (!confirmed) return;
    busyServerId.value = item.config.id;
    try {
      overview.value = await deleteUpstreamMcpServer(item.config.id);
      if (toolsModalServerId.value === item.config.id) {
        toolsModalServerId.value = null;
      }
      showToast(`已移除「${item.config.name}」`, { kind: "success" });
    } catch (error) {
      showToast(`移除失败：${String(error)}`, { kind: "error" });
    } finally {
      busyServerId.value = null;
    }
  }

  function openToolsModal(item: UpstreamServerOverviewDto) {
    toolsModalServerId.value = item.config.id;
  }

  async function saveDisabledTools(serverId: string, disabledTools: string[]) {
    toolsSaving.value = true;
    try {
      overview.value = await updateUpstreamMcpDisabledTools(serverId, disabledTools);
      showToast("已保存工具启停过滤配置", { kind: "success" });
      toolsModalServerId.value = null;
    } catch (error) {
      showToast(`保存失败：${String(error)}`, { kind: "error" });
    } finally {
      toolsSaving.value = false;
    }
  }

  async function openImportModal(tab: "scan" | "json" = "scan") {
    importTab.value = tab;
    importError.value = "";
    replaceSourceConfigs.value = false;
    takeoverPreview.value = [];
    importOpen.value = true;
    if (tab === "scan") {
      await runLocalScan();
    }
  }

  async function runLocalScan() {
    scanningClients.value = true;
    importError.value = "";
    try {
      const items = await scanLocalUpstreamMcpClients();
      scannedCandidates.value = items;
      initSelections(items);
    } catch (error) {
      importError.value = String(error);
    } finally {
      scanningClients.value = false;
    }
  }

  async function runParseJson() {
    if (!importJsonText.value.trim()) {
      importError.value = "请粘贴标准 mcpServers JSON 配置";
      return;
    }
    parsingJson.value = true;
    importError.value = "";
    try {
      const items = await parseUpstreamMcpJson(importJsonText.value);
      parsedCandidates.value = items;
      initSelections(items);
    } catch (error) {
      parsedCandidates.value = [];
      importError.value = String(error);
    } finally {
      parsingJson.value = false;
    }
  }

  async function submitImport() {
    const selections = buildCurrentImportSelections();
    if (selections.length === 0) {
      importError.value = "请至少勾选一个有效条目再导入";
      return;
    }
    const shouldReplace = importTab.value === "scan" && replaceSourceConfigs.value;
    if (shouldReplace) {
      const targetNames = takeoverPreview.value.map((p) => `${p.sourceLabel} (${p.displayPath})`).join("、");
      const confirmed = await ask(
        `即将导入 ${selections.length} 个上游 MCP，并自动备份后改写以下客户端配置：${targetNames || "所选客户端"}。\n\n被接管的旧 MCP 条目将替换为统一的 Coding Tools MCP 入口，且随时可在“MCP 聚合中心”一键还原。确定继续？`,
        { title: "确认聚合并替换客户端配置", kind: "warning" },
      );
      if (!confirmed) return;
    }
    importingBusy.value = true;
    importError.value = "";
    try {
      const rawJson = importTab.value === "json" ? importJsonText.value : null;
      const res = await importUpstreamMcpCandidates(rawJson, selections, {
        replaceSourceConfigs: shouldReplace,
        connectMode: takeoverConnectMode.value,
      });
      overview.value = res.overview;
      if (shouldReplace) {
        takeoverBackups.value = await listUpstreamMcpTakeoverBackups().catch(() => takeoverBackups.value);
      }
      importOpen.value = false;
      importJsonText.value = "";
      parsedCandidates.value = [];
      replaceSourceConfigs.value = false;
      takeoverPreview.value = [];

      if (shouldReplace && res.replaceErrors.length > 0) {
        showToast(
          `已导入 ${res.importedCount} 项、接管 ${res.replacedFiles.length} 个客户端，但部分替换失败：${res.replaceErrors[0]}`,
          { kind: "warning" },
        );
      } else if (shouldReplace && res.replacedFiles.length > 0) {
        showToast(
          `聚合并替换完成：已导入 ${res.importedCount} 项并接管 ${res.replacedFiles.length} 个客户端（原配置已自动备份，可随时还原）`,
          { kind: "success" },
        );
      } else if (res.skipped.length > 0) {
        showToast(
          `成功导入 ${res.importedCount} 个上游 MCP，跳过 ${res.skipped.length} 个`,
          { kind: "warning" },
        );
      } else {
        showToast(`成功导入 ${res.importedCount} 个上游 MCP，可点击“全部测试同步”拉取工具`, { kind: "success" });
      }
    } catch (error) {
      importError.value = String(error);
    } finally {
      importingBusy.value = false;
    }
  }

  onMounted(() => {
    void refresh();
  });

  return {
    loading,
    loadError,
    overview,
    globalBusy,
    syncingAll,
    busyServerId,
    testingServerId,
    directToolsOverLimit,
    editorOpen,
    editorSaving,
    editorError,
    editorDraft,
    toolsModalServerId,
    toolsModalServer,
    toolsSaving,
    importOpen,
    importTab,
    scanningClients,
    parsingJson,
    importingBusy,
    importError,
    importJsonText,
    activeImportCandidates,
    candidateSelections,
    replaceSourceConfigs,
    takeoverConnectMode,
    takeoverPreview,
    loadingTakeoverPreview,
    takeoverBackups,
    restoringBackupId,
    refresh,
    restoreBackup,
    updateGlobalSettings,
    openCreateModal,
    openEditModal,
    submitEditor,
    toggleServerEnabled,
    switchServerMode,
    testAndSyncServer,
    syncAllEnabled,
    removeServer,
    openToolsModal,
    saveDisabledTools,
    openImportModal,
    runLocalScan,
    runParseJson,
    submitImport,
  };
}
