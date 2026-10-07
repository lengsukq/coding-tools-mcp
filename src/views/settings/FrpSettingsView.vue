<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Edit3, RadioTower, Save, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ConfirmDialog from "$src/components/ui/ConfirmDialog.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import TextField from "$src/components/ui/TextField.vue";
import {
  deleteFrpProfile,
  listFrpProfiles,
  saveFrpProfile,
  type FrpProfileDto,
} from "$lib/api/settings";
import SaveBar from "$src/components/ui/SaveBar.vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import { showToast } from "$lib/stores/toast";

const profiles = ref<FrpProfileDto[]>([]);
const loading = ref(true);
const saving = ref(false);
const editingId = ref<string | null>(null);
const name = ref("");
const server = ref("");
const serverPort = ref(7000);
const token = ref("");
const pendingDelete = ref<FrpProfileDto | null>(null);
const deleteBusy = ref(false);
const error=ref("");const loadError=ref("");
const form=()=>JSON.stringify([name.value,server.value,serverPort.value,token.value]);
const baseline=ref(form());const dirty=computed(()=>form()!==baseline.value);

async function refresh() {
  loading.value = true;loadError.value="";
  try { profiles.value = await listFrpProfiles(); }
  catch (e) { loadError.value=String(e); }
  finally { loading.value = false; }
}

function resetForm() {
  editingId.value = null;
  name.value = "";
  server.value = "";
  serverPort.value = 7000;
  token.value = "";
  baseline.value=form();error.value="";
}

function edit(profile: FrpProfileDto) {
  if(dirty.value || saving.value)return;
  editingId.value = profile.id;
  name.value = profile.name;
  server.value = profile.server;
  serverPort.value = profile.serverPort;
  token.value = "";
  baseline.value=form();error.value="";
}

async function save() {
  if (!name.value.trim() || !server.value.trim()) {
    error.value="请填写配置名称和服务器地址。";return false;
  }
  if (!Number.isInteger(Number(serverPort.value)) || Number(serverPort.value) < 1 || Number(serverPort.value) > 65535) {
    error.value="端口必须是 1–65535 的整数。";return false;
  }
  if(saving.value)return false;
  error.value="";saving.value = true;
  try {
    await saveFrpProfile({
      id: editingId.value ?? "",
      name: name.value.trim(),
      server: server.value.trim(),
      serverPort: Number(serverPort.value),
    }, token.value.trim() || undefined);
    resetForm();
    await refresh();
    showToast("FRP 配置已保存", { kind: "success" });return true;
  } catch (e) {
    error.value=String(e);return false;
  } finally { saving.value = false; }
}

async function remove() {
  if (!pendingDelete.value || deleteBusy.value) return;
  deleteBusy.value = true;
  try {
    await deleteFrpProfile(pendingDelete.value.id);
    if (editingId.value === pendingDelete.value.id) resetForm();
    pendingDelete.value = null;
    await refresh();
    showToast("FRP 配置已删除", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "删除失败", kind: "error" });
  } finally { deleteBusy.value = false; }
}

useUnsavedChanges("FRP 配置",dirty,save);
onMounted(() => void refresh());
</script>

<template>
  <div class="ui-stack">
    <div class="frp-settings-grid animate-fade-in-up delay-100">
      <GlassCard>
        <div class="mb-5 flex items-center gap-3">
          <div class="grid h-10 w-10 place-items-center rounded-[var(--ui-radius-row)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]"><RadioTower :size="18" /></div>
          <div><h2 class="text-base font-semibold">{{ editingId ? "编辑配置" : "新建配置" }}</h2><p class="mt-0.5 text-xs text-[var(--ui-text-muted)]">由全局连接选择此配置。</p></div>
        </div>
        <fieldset :disabled="saving"><form class="grid gap-3.5" @submit.prevent="save">
          <TextField v-model="name" label="名称" placeholder="公司 FRP / 家庭内网穿透" />
          <TextField v-model="server" label="服务器域名" placeholder="frp.example.com" />
          <TextField :model-value="String(serverPort)" label="端口" type="number" @update:model-value="serverPort = Number($event)" />
          <TextField v-model="token" :label="`Token ${editingId ? '（留空保持不变）' : '（可选）'}`" type="password" placeholder="frp auth token" />
          <SaveBar :dirty="dirty" :busy="saving" :error="error" :label="editingId ? '更新配置' : '添加配置'" @save="save"/><BaseButton v-if="editingId || dirty" variant="ghost" :disabled="saving" @click="resetForm">{{editingId ? "取消编辑" : "放弃草稿"}}</BaseButton>
        </form></fieldset>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-base font-semibold">已保存的配置</h2><p class="mt-1 text-xs text-[var(--ui-text-muted)]">删除配置不会删除工作区，但使用它的全局 FRP 隧道将无法连接。</p></div>
        <p v-if="loading" class="py-8 text-center text-xs text-[var(--text-muted)]">加载中…</p>
        <div v-else-if="loadError" class="ui-banner ui-banner--warning" role="alert"><span>{{loadError}}</span><BaseButton variant="secondary" @click="refresh">重试</BaseButton></div><p v-else-if="profiles.length === 0" class="py-8 text-center text-xs text-[var(--text-muted)]">暂无 FRP 配置。</p>
        <div v-else class="space-y-2">
          <div v-for="profile in profiles" :key="profile.id" class="frp-profile-row ui-inset p-3.5">
            <div class="min-w-0"><p class="truncate text-sm font-medium">{{ profile.name }}</p><p class="mt-1 truncate font-mono text-xs text-[var(--ui-text-muted)]">{{ profile.server }}:{{ profile.serverPort }} · Token {{ profile.hasToken ? "已配置" : "未配置" }}</p></div>
            <div class="flex gap-1"><BaseButton variant="ghost" size="sm" :disabled="dirty || saving" @click="edit(profile)"><Edit3 :size="12" />编辑</BaseButton><BaseButton variant="danger" size="sm" :disabled="dirty || saving || deleteBusy" @click="pendingDelete = profile"><Trash2 :size="12" />删除</BaseButton></div>
          </div>
        </div>
      </GlassCard>
    </div>
  </div>

  <ConfirmDialog
    :open="!!pendingDelete"
    title="删除 FRP 配置"
    :message="pendingDelete ? `确定删除配置「${pendingDelete.name}」？使用此配置的全局公网入口将无法建立 FRP 隧道。` : ''"
    :detail="pendingDelete ? `${pendingDelete.server}:${pendingDelete.serverPort}` : undefined"
    confirm-text="确认删除"
    severity="danger"
    :busy="deleteBusy"
    @confirm="remove"
    @cancel="pendingDelete = null"
  />
</template>

<style scoped>
.settings-page {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.frp-settings-grid {
  display: grid;
  grid-template-columns: minmax(300px, 420px) minmax(0, 1fr);
  gap: 1rem;
  min-width: 0;
}

.frp-profile-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: .75rem;
  min-width: 0;
}

@container app-main (max-width: 900px) {
  .frp-settings-grid {
    grid-template-columns: 1fr;
  }
}

@container app-main (max-width: 640px) {
  .settings-page {
    padding-inline: 16px;
  }

  .frp-profile-row {
    flex-wrap: wrap;
    align-items: flex-start;
  }
}
</style>
