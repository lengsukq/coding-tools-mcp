<script setup lang="ts">
import LanLocalDeviceCard from "$src/components/settings/LanLocalDeviceCard.vue";
import LanPeersDiscoveryCard from "$src/components/settings/LanPeersDiscoveryCard.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import { useLanPeers } from "$lib/lan/use-lan-peers";

withDefaults(
  defineProps<{
    panel?: "lan" | "lan-peers";
  }>(),
  { panel: "lan" },
);

const {
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
} = useLanPeers();
</script>

<template>
  <section id="settings-lan" class="ui-stack" aria-label="局域网多设备配对">
    <div v-if="loadError" class="ui-banner ui-banner--warning" role="alert">
      <span>读取局域网配对状态失败：{{ loadError }}</span>
      <BaseButton variant="secondary" size="sm" @click="refresh(true)">重试</BaseButton>
    </div>

    <p v-else-if="loading && !overview" role="status" class="ui-muted">正在加载局域网设备信息…</p>

    <template v-if="overview">
      <LanPeersDiscoveryCard
        v-if="panel === 'lan-peers'"
        :discovered-peers="discoveredUnpaired"
        :paired-peers="overview.pairedPeers"
        :scanning="scanning"
        :probing="probing"
        v-model:probe-host="probeHost"
        v-model:probe-port="probePort"
        :probe-error="probeError"
        :peer-action-id="peerActionId"
        :pairing-modal-open="pairingModalOpen"
        :pairing-target="pairingTarget"
        v-model:pin-input="pinInput"
        :pair-error="pairError"
        :pairing-busy="pairingBusy"
        :rename-modal-open="renameModalOpen"
        :renaming-busy="renamingBusy"
        v-model:rename-input="renameInput"
        @scan="scanNearby"
        @probe="probeManualAddress"
        @open-pair="openPairModal"
        @close-pair="closePairModal"
        @submit-pair="submitPair"
        @toggle-peer="togglePeerEnabled"
        @open-rename="openRenameModal"
        @close-rename="renameModalOpen = false"
        @submit-rename="submitRename"
        @unpair="unpairPeer"
      />

      <LanLocalDeviceCard
        v-else
        :local-device="overview.localDevice"
        :mcp-running="overview.mcpRunning"
        :online-peer-count="onlinePeerCount"
        :paired-total-count="overview.pairedPeers.length"
        :draft="draft"
        :workspaces="workspaces"
        :dirty="dirty"
        :saving="saving"
        :save-error="saveError"
        :active-pin="activePin"
        :pin-countdown-sec="pinCountdownSec"
        :pin-busy="pinBusy"
        :copied-key="copiedKey"
        @update-draft="patchDraft"
        @save="saveSettings"
        @generate-pin="generatePin"
        @revoke-pin="revokePin"
        @toggle-workspace="toggleSharedWorkspace"
        @copy-text="copyText"
      />
    </template>
  </section>
</template>
