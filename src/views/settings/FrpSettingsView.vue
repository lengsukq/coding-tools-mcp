<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Edit3, RadioTower, Save, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ConfirmDialog from "$src/components/ui/ConfirmDialog.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import TextField from "$src/components/ui/TextField.vue";
import ConnectionSettingsNav from "$src/components/settings/ConnectionSettingsNav.vue";
import SettingsPageHeader from "$src/components/settings/SettingsPageHeader.vue";
import {
  deleteFrpProfile,
  listFrpProfiles,
  saveFrpProfile,
  type FrpProfileDto,
} from "$lib/api/settings";
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

async function refresh() {
  loading.value = true;
  try { profiles.value = await listFrpProfiles(); }
  catch (error) { showToast(String(error), { title: "加载 FRP 配置失败", kind: "error" }); }
  finally { loading.value = false; }
}

function resetForm() {
  editingId.value = null;
  name.value = "";
  server.value = "";
  serverPort.value = 7000;
  token.value = "";
}

function edit(profile: FrpProfileDto) {
  editingId.value = profile.id;
  name.value = profile.name;
  server.value = profile.server;
  serverPort.value = profile.serverPort;
  token.value = "";
}

async function save() {
  if (!name.value.trim() || !server.value.trim()) {
    showToast("请填写配置名称和服务器地址。", { title: "无法保存", kind: "warning" });
    return;
  }
  if (!Number.isInteger(Number(serverPort.value)) || Number(serverPort.value) < 1 || Number(serverPort.value) > 65535) {
    showToast("端口必须是 1-65535 之间的整数。", { title: "无法保存", kind: "warning" });
    return;
  }
  saving.value = true;
  try {
    await saveFrpProfile({
      id: editingId.value ?? "",
      name: name.value.trim(),
      server: server.value.trim(),
      serverPort: Number(serverPort.value),
    }, token.value.trim() || undefined);
    resetForm();
    await refresh();
    showToast("FRP 配置已保存", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "保存失败", kind: "error" });
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

onMounted(() => void refresh());
</script>

<template>
  <div class="mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="FRP 配置" description="集中管理 FRP 服务器与 Token，各 Workspace 只需选择 Profile 并配置自己的子域名。" />
    <ConnectionSettingsNav />
    <div class="grid gap-4 lg:grid-cols-[420px_minmax(0,1fr)]">
      <GlassCard>
        <div class="mb-5 flex items-center gap-3">
          <div class="grid h-10 w-10 place-items-center rounded-2xl bg-[#64d2ff]/14 text-[#0a84ff]"><RadioTower :size="18" /></div>
          <div><h2 class="text-sm font-semibold">{{ editingId ? "编辑配置" : "新建配置" }}</h2><p class="mt-0.5 text-[11px] text-[var(--text-muted)]">Profile 可被多个 Workspace 复用。</p></div>
        </div>
        <form class="grid gap-3.5" @submit.prevent="save">
          <TextField v-model="name" label="名称" placeholder="公司 FRP / 家庭内网穿透" />
          <TextField v-model="server" label="服务器域名" placeholder="frp.example.com" />
          <TextField :model-value="String(serverPort)" label="端口" type="number" @update:model-value="serverPort = Number($event)" />
          <TextField v-model="token" :label="`Token ${editingId ? '（留空保持不变）' : '（可选）'}`" type="password" placeholder="frp auth token" />
          <div class="flex gap-2 pt-1"><BaseButton :busy="saving" @click="save"><Save :size="14" />{{ editingId ? "更新配置" : "添加配置" }}</BaseButton><BaseButton v-if="editingId" variant="ghost" @click="resetForm">取消</BaseButton></div>
        </form>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold">已保存的配置</h2><p class="mt-1 text-xs text-[var(--text-muted)]">删除 Profile 不会删除 Workspace，但依赖它的 FRP Tunnel 将无法连接。</p></div>
        <p v-if="loading" class="py-8 text-center text-xs text-[var(--text-muted)]">加载中…</p>
        <p v-else-if="profiles.length === 0" class="py-8 text-center text-xs text-[var(--text-muted)]">暂无 FRP 配置。</p>
        <div v-else class="space-y-2">
          <div v-for="profile in profiles" :key="profile.id" class="ios-glass flex items-center justify-between gap-3 rounded-2xl p-3.5">
            <div class="min-w-0"><p class="truncate text-xs font-semibold">{{ profile.name }}</p><p class="mt-1 truncate font-mono text-[10px] text-[var(--text-muted)]">{{ profile.server }}:{{ profile.serverPort }} · Token {{ profile.hasToken ? "已配置" : "未配置" }}</p></div>
            <div class="flex gap-1"><BaseButton variant="ghost" size="sm" @click="edit(profile)"><Edit3 :size="12" />编辑</BaseButton><BaseButton variant="danger" size="sm" @click="pendingDelete = profile"><Trash2 :size="12" />删除</BaseButton></div>
          </div>
        </div>
      </GlassCard>
    </div>
  </div>

  <ConfirmDialog
    :open="!!pendingDelete"
    title="删除 FRP 配置"
    :message="pendingDelete ? `确定删除配置「${pendingDelete.name}」？依赖此 Profile 的 Workspace 将无法建立 FRP 隧道。` : ''"
    :detail="pendingDelete ? `${pendingDelete.server}:${pendingDelete.serverPort}` : undefined"
    confirm-text="确认删除"
    severity="danger"
    :busy="deleteBusy"
    @confirm="remove"
    @cancel="pendingDelete = null"
  />
</template>
