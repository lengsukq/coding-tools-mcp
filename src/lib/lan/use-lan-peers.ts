import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  cancelLanPairingPin,
  createLanPairingPin,
  getLanPeersOverview,
  pairLanPeer,
  probeLanPeerAddress,
  removeLanPairedPeer,
  renameLanPairedPeer,
  saveLanFederationSettings,
  scanLanPeers,
  setLanPairedPeerEnabled,
  type DiscoveredPeerDto,
  type LanOverviewDto,
  type LanSettingsUpdateDto,
  type PairedPeerStatusDto,
  type PairingPinDto,
} from "$lib/api/lan-peers";
import { useClipboard } from "$lib/composables/use-clipboard";
import { workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import { isValidPort } from "$lib/validators";

const DEFAULT_LAN_MCP_PORT = 28765;
const PAIRING_PIN_LENGTH = 6;
const PIN_COUNTDOWN_TICK_MS = 1000;
const COPY_FEEDBACK_MS = 1800;

export function formatPlatformLabel(os: string, arch?: string): string {
  const osLabel =
    os === "macos" ? "macOS" : os === "windows" ? "Windows" : os === "linux" ? "Linux" : os || "未知系统";
  if (!arch) return osLabel;
  const archLabel = arch === "aarch64" ? "Apple Silicon / ARM64" : arch === "x86_64" ? "x64" : arch;
  return `${osLabel} · ${archLabel}`;
}

export function parseExtraSubnetsText(raw: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const token of raw.split(/[,;\n\r]+/)) {
    const clean = token.trim();
    if (clean && !seen.has(clean)) {
      seen.add(clean);
      out.push(clean);
    }
  }
  return out;
}

export interface LanDraftForm {
  enabled: boolean;
  deviceName: string;
  shareMode: string;
  sharedWorkspaceIds: string[];
  trustLevel: string;
  allowComputerUse: boolean;
  allowLocalNoauth: boolean;
  extraSubnetsText: string;
}

function cloneLanDraft(source: LanDraftForm): LanDraftForm {
  return {
    ...source,
    sharedWorkspaceIds: [...source.sharedWorkspaceIds],
  };
}

function isLanDraftEqual(a: LanDraftForm, b: LanDraftForm): boolean {
  if (
    a.enabled !== b.enabled ||
    a.deviceName !== b.deviceName ||
    a.shareMode !== b.shareMode ||
    a.trustLevel !== b.trustLevel ||
    a.allowComputerUse !== b.allowComputerUse ||
    a.allowLocalNoauth !== b.allowLocalNoauth ||
    a.extraSubnetsText !== b.extraSubnetsText
  ) {
    return false;
  }
  if (a.sharedWorkspaceIds.length !== b.sharedWorkspaceIds.length) return false;
  return a.sharedWorkspaceIds.every((id, index) => id === b.sharedWorkspaceIds[index]);
}

export function useLanPeers() {
  const loading = ref(true);
  const scanning = ref(false);
  const saving = ref(false);
  const pinBusy = ref(false);
  const probing = ref(false);
  const pairingBusy = ref(false);
  const renamingBusy = ref(false);
  const peerActionId = ref<string | null>(null);
  const loadError = ref("");
  const saveError = ref("");
  const probeError = ref("");
  const pairError = ref("");

  const overview = ref<LanOverviewDto | null>(null);
  const activePin = ref<PairingPinDto | null>(null);
  const pinCountdownSec = ref(0);
  const { copiedKey: clipboardKey, copyText: copyToClipboard } = useClipboard(COPY_FEEDBACK_MS);
  const copiedKey = computed<string | null>(() => clipboardKey.value || null);

  const draft = reactive<LanDraftForm>({
    enabled: false,
    deviceName: "",
    shareMode: "all",
    sharedWorkspaceIds: [],
    trustLevel: "full",
    allowComputerUse: true,
    allowLocalNoauth: true,
    extraSubnetsText: "",
  });
  const baseline = ref<LanDraftForm | null>(null);

  const probeHost = ref("");
  const probePort = ref(String(DEFAULT_LAN_MCP_PORT));

  const pairingModalOpen = ref(false);
  const pairingTarget = ref<{
    deviceName: string;
    host: string;
    port: number;
    os: string;
    fingerprint: string;
  } | null>(null);
  const pinInput = ref("");

  const renameModalOpen = ref(false);
  const renameTargetId = ref("");
  const renameInput = ref("");

  let loadSequence = 0;
  let pinTimer: number | null = null;

  const dirty = computed(() => !loading.value && baseline.value !== null && !isLanDraftEqual(draft, baseline.value));
  const discoveredUnpaired = computed(() =>
    (overview.value?.discoveredPeers ?? []).filter((item) => !item.alreadyPaired),
  );
  const onlinePeerCount = computed(() =>
    (overview.value?.pairedPeers ?? []).filter((item) => item.enabled && item.online).length,
  );

  function patchDraft(patch: Partial<LanDraftForm>) {
    Object.assign(draft, patch);
  }

  function stopPinTimer() {
    if (pinTimer !== null) {
      window.clearInterval(pinTimer);
      pinTimer = null;
    }
  }

  function syncPinState(pin: PairingPinDto | null) {
    stopPinTimer();
    activePin.value = pin;
    if (!pin) {
      pinCountdownSec.value = 0;
      return;
    }
    const updateRemaining = () => {
      const remaining = Math.max(0, Math.ceil((pin.expiresAtMs - Date.now()) / 1000));
      pinCountdownSec.value = remaining;
      if (remaining <= 0) {
        activePin.value = null;
        stopPinTimer();
      }
    };
    updateRemaining();
    if (pinCountdownSec.value > 0) {
      pinTimer = window.setInterval(updateRemaining, PIN_COUNTDOWN_TICK_MS);
    }
  }

  function applyOverview(next: LanOverviewDto, overwriteDraft = !dirty.value) {
    overview.value = next;
    syncPinState(next.activePin);
    if (!probePort.value || probePort.value === String(DEFAULT_LAN_MCP_PORT)) {
      probePort.value = String(next.localDevice.mcpPort || DEFAULT_LAN_MCP_PORT);
    }
    if (overwriteDraft) {
      draft.enabled = next.config.enabled;
      draft.deviceName = next.localDevice.deviceName;
      draft.shareMode = next.config.shareMode || "all";
      draft.sharedWorkspaceIds = [...(next.config.sharedWorkspaceIds ?? [])];
      draft.trustLevel = next.config.trustLevel || "full";
      draft.allowComputerUse = next.config.allowComputerUse ?? true;
      draft.allowLocalNoauth = next.localDevice.allowLocalNoauth ?? true;
      draft.extraSubnetsText = (next.config.extraSubnets ?? []).join("\n");
      baseline.value = cloneLanDraft(draft);
    }
  }

  async function refresh(forceRefresh = false) {
    const seq = ++loadSequence;
    loading.value = true;
    loadError.value = "";
    try {
      const next = await getLanPeersOverview(forceRefresh);
      if (seq !== loadSequence) return;
      applyOverview(next);
    } catch (error) {
      if (seq === loadSequence) {
        loadError.value = String(error);
      }
    } finally {
      if (seq === loadSequence) {
        loading.value = false;
      }
    }
  }

  async function saveSettings(): Promise<boolean> {
    if (saving.value) return false;
    if (!dirty.value) return true;
    if (!draft.deviceName.trim()) {
      saveError.value = "请填写本机在局域网中显示的设备名称。";
      return false;
    }
    saving.value = true;
    saveError.value = "";
    try {
      const payload: LanSettingsUpdateDto = {
        enabled: draft.enabled,
        deviceName: draft.deviceName.trim(),
        shareMode: draft.shareMode,
        sharedWorkspaceIds: [...draft.sharedWorkspaceIds],
        trustLevel: draft.trustLevel,
        allowComputerUse: draft.allowComputerUse,
        extraSubnets: parseExtraSubnetsText(draft.extraSubnetsText),
        allowLocalNoauth: draft.allowLocalNoauth,
      };
      const res = await saveLanFederationSettings(payload);
      applyOverview(res.overview, true);
      if (res.listenerRestarted) {
        showToast("局域网与组网配置已保存，并自动同步重启本地 MCP 监听。", { kind: "success", duration: 4500 });
      } else {
        showToast("局域网与组网配置已保存。", { kind: "success" });
      }
      return true;
    } catch (error) {
      saveError.value = String(error);
      return false;
    } finally {
      saving.value = false;
    }
  }

  function toggleSharedWorkspace(id: string, checked: boolean) {
    const set = new Set(draft.sharedWorkspaceIds);
    if (checked) set.add(id);
    else set.delete(id);
    draft.sharedWorkspaceIds = [...set];
  }

  async function generatePin() {
    if (pinBusy.value) return;
    pinBusy.value = true;
    try {
      if (!draft.enabled) {
        draft.enabled = true;
        const ok = await saveSettings();
        if (!ok) return;
      }
      const pin = await createLanPairingPin();
      syncPinState(pin);
      showToast("已生成 6 位一次性配对码（120 秒内有效），请在另一台电脑输入。", { kind: "info", duration: 4000 });
    } catch (error) {
      showToast(String(error), { title: "生成配对码失败", kind: "error" });
    } finally {
      pinBusy.value = false;
    }
  }

  async function revokePin() {
    if (pinBusy.value) return;
    pinBusy.value = true;
    try {
      await cancelLanPairingPin();
      syncPinState(null);
    } catch (error) {
      showToast(String(error), { title: "取消配对码失败", kind: "error" });
    } finally {
      pinBusy.value = false;
    }
  }

  async function scanNearby() {
    if (scanning.value) return;
    scanning.value = true;
    try {
      if (!draft.enabled || dirty.value) {
        draft.enabled = true;
        const ok = await saveSettings();
        if (!ok) return;
      }
      const next = await scanLanPeers();
      applyOverview(next);
      showToast("已刷新局域网 / 组网设备与在线状态", { kind: "success", duration: 1800 });
    } catch (error) {
      showToast(String(error), { title: "扫描节点失败", kind: "error" });
    } finally {
      scanning.value = false;
    }
  }

  async function probeManualAddress() {
    if (probing.value) return;
    probeError.value = "";
    const host = probeHost.value.trim();
    const port = Number(probePort.value);
    if (!host) {
      probeError.value = "请输入对方电脑的局域网或 EasyTier/Tailscale 组网 IP（例如 10.144.144.2）";
      return;
    }
    if (!isValidPort(port)) {
      probeError.value = "端口必须是 1 到 65535 之间的整数";
      return;
    }
    probing.value = true;
    try {
      const peer = await probeLanPeerAddress(host, port);
      await refresh(false);
      if (peer.alreadyPaired) {
        showToast(`设备 ${peer.deviceName} 已在配对列表中，组网/局域网地址已同步更新。`, { kind: "success" });
      } else {
        openPairModal(peer);
      }
    } catch (error) {
      probeError.value = String(error);
    } finally {
      probing.value = false;
    }
  }

  function openPairModal(peer: DiscoveredPeerDto) {
    pairingTarget.value = {
      deviceName: peer.deviceName,
      host: peer.host,
      port: peer.port,
      os: peer.os,
      fingerprint: peer.fingerprint,
    };
    pinInput.value = "";
    pairError.value = "";
    pairingModalOpen.value = true;
  }

  function closePairModal() {
    if (pairingBusy.value) return;
    pairingModalOpen.value = false;
    pairingTarget.value = null;
    pinInput.value = "";
    pairError.value = "";
  }

  async function submitPair() {
    if (pairingBusy.value || !pairingTarget.value) return;
    const digits = pinInput.value.replace(/\D/g, "");
    if (digits.length !== PAIRING_PIN_LENGTH) {
      pairError.value = "请输入对方电脑上显示的 6 位数字配对码";
      return;
    }
    pairingBusy.value = true;
    pairError.value = "";
    try {
      const targetName = pairingTarget.value.deviceName;
      const next = await pairLanPeer(pairingTarget.value.host, pairingTarget.value.port, digits);
      applyOverview(next, true);
      pairingModalOpen.value = false;
      pairingTarget.value = null;
      pinInput.value = "";
      showToast(`已与「${targetName}」完成双向配对，任意一端均可直接访问对方工作区。`, {
        kind: "success",
        duration: 5000,
      });
    } catch (error) {
      pairError.value = String(error);
    } finally {
      pairingBusy.value = false;
    }
  }

  async function togglePeerEnabled(peer: PairedPeerStatusDto, enabled: boolean) {
    peerActionId.value = peer.deviceId;
    try {
      const next = await setLanPairedPeerEnabled(peer.deviceId, enabled);
      applyOverview(next);
      showToast(enabled ? `已启用设备 ${peer.deviceName}` : `已暂停设备 ${peer.deviceName} 联邦路由`, {
        kind: "info",
      });
    } catch (error) {
      showToast(String(error), { title: "更新设备状态失败", kind: "error" });
    } finally {
      peerActionId.value = null;
    }
  }

  function openRenameModal(peer: PairedPeerStatusDto) {
    renameTargetId.value = peer.deviceId;
    renameInput.value = peer.deviceName;
    renameModalOpen.value = true;
  }

  async function submitRename() {
    const id = renameTargetId.value;
    const name = renameInput.value.trim();
    if (!id || !name || renamingBusy.value) return;
    renamingBusy.value = true;
    peerActionId.value = id;
    try {
      const next = await renameLanPairedPeer(id, name);
      applyOverview(next);
      renameModalOpen.value = false;
      showToast("设备备注名称已更新", { kind: "success" });
    } catch (error) {
      showToast(String(error), { title: "重命名失败", kind: "error" });
    } finally {
      renamingBusy.value = false;
      peerActionId.value = null;
    }
  }

  async function unpairPeer(peer: PairedPeerStatusDto) {
    const confirmed = await ask(
      `解除与「${peer.deviceName}」（${peer.host}:${peer.port}）的配对后，双方将立即撤销共享密钥并停止跨设备工作区访问。`,
      {
        title: "解除局域网设备配对",
        kind: "warning",
        okLabel: "解除配对",
        cancelLabel: "取消",
      },
    );
    if (!confirmed) return;
    peerActionId.value = peer.deviceId;
    try {
      const next = await removeLanPairedPeer(peer.deviceId);
      applyOverview(next);
      showToast(`已解除与 ${peer.deviceName} 的配对`, { kind: "info" });
    } catch (error) {
      showToast(String(error), { title: "解除配对失败", kind: "error" });
    } finally {
      peerActionId.value = null;
    }
  }

  async function copyText(text: string, key: string, label: string) {
    await copyToClipboard(text, { key, label, durationMs: COPY_FEEDBACK_MS });
  }

  useUnsavedChanges("局域网多设备配对", dirty, saveSettings);

  onMounted(() => {
    void refresh(false);
  });

  onBeforeUnmount(() => {
    loadSequence++;
    stopPinTimer();
  });

  return {
    workspaces,
    loading,
    scanning,
    saving,
    pinBusy,
    probing,
    pairingBusy,
    renamingBusy,
    peerActionId,
    loadError,
    saveError,
    probeError,
    pairError,
    overview,
    activePin,
    pinCountdownSec,
    copiedKey,
    draft,
    dirty,
    discoveredUnpaired,
    onlinePeerCount,
    probeHost,
    probePort,
    pairingModalOpen,
    pairingTarget,
    pinInput,
    renameModalOpen,
    renameInput,
    patchDraft,
    refresh,
    saveSettings,
    toggleSharedWorkspace,
    generatePin,
    revokePin,
    scanNearby,
    probeManualAddress,
    openPairModal,
    closePairModal,
    submitPair,
    togglePeerEnabled,
    openRenameModal,
    submitRename,
    unpairPeer,
    copyText,
  };
}
