<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Download, Package, RefreshCw, Save, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ConnectionSettingsNav from "$src/components/settings/ConnectionSettingsNav.vue";
import SettingsPageHeader from "$src/components/settings/SettingsPageHeader.vue";
import {
  getDownloadConfig,
  installSoftware,
  listSoftware,
  setDownloadConfig,
  uninstallSoftware,
  type DownloadConfig,
  type SoftwareStatus,
} from "$lib/api/software";
import { showToast } from "$lib/stores/toast";

const software = ref<SoftwareStatus[]>([]);
const loading = ref(true);
const installing = ref<string | null>(null);
const uninstalling = ref<string | null>(null);
const saving = ref(false);
const config = reactive<DownloadConfig>({ githubMirror: "https://gh-proxy.com", proxyMode: "system", proxyUrl: "" });
const baseline = ref("");
const dirty = computed(() => JSON.stringify(config) !== baseline.value);
const proxyOptions = [
  { value: "system", label: "系统代理（默认）" },
  { value: "none", label: "无代理" },
  { value: "manual", label: "手动代理地址" },
];

async function refresh() {
  loading.value = true;
  try {
    const [nextSoftware, nextConfig] = await Promise.all([listSoftware(), getDownloadConfig()]);
    software.value = nextSoftware;
    Object.assign(config, nextConfig);
    baseline.value = JSON.stringify(config);
  } catch (error) { showToast(String(error), { title: "加载软件状态失败", kind: "error" }); }
  finally { loading.value = false; }
}

async function install(kind: string) {
  installing.value = kind;
  try { await installSoftware(kind); await refresh(); showToast("软件安装完成", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "安装失败", kind: "error" }); }
  finally { installing.value = null; }
}

async function uninstall(kind: string) {
  uninstalling.value = kind;
  try { await uninstallSoftware(kind); await refresh(); showToast("软件已卸载", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "卸载失败", kind: "error" }); }
  finally { uninstalling.value = null; }
}

async function saveConfig() {
  if (!dirty.value || saving.value) return;
  saving.value = true;
  try { await setDownloadConfig({ ...config }); baseline.value = JSON.stringify(config); showToast("下载配置已保存", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "保存失败", kind: "error" }); }
  finally { saving.value = false; }
}

onMounted(() => void refresh());
</script>

<template>
  <div class="settings-page mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="软件管理" description="管理 frpc 与 cloudflared 隧道客户端，以及下载镜像和代理策略。" />
    <ConnectionSettingsNav />
    <div class="grid gap-4">
      <GlassCard>
        <div class="mb-4 flex items-center justify-between"><div><h2 class="text-sm font-semibold">隧道客户端</h2><p class="mt-1 text-xs text-[var(--text-muted)]">应用托管的二进制会放在缓存目录，不覆盖系统安装。</p></div><BaseButton variant="ghost" size="sm" :busy="loading" @click="refresh"><RefreshCw :size="13" />刷新</BaseButton></div>
        <div class="software-grid">
          <div v-for="item in software" :key="item.kind" class="ios-glass flex items-center gap-3 rounded-2xl p-3.5">
            <div class="grid h-10 w-10 place-items-center rounded-2xl bg-[#5e5ce6]/12 text-[#5e5ce6]"><Package :size="18" /></div>
            <div class="min-w-0 flex-1"><p class="text-xs font-semibold">{{ item.name }}</p><p class="mt-1 truncate font-mono text-[10px] text-[var(--text-muted)]">{{ item.installed ? item.path : "未安装" }} · {{ item.managed ? "应用托管" : "系统安装" }}</p></div>
            <BaseButton v-if="!item.installed" size="sm" :busy="installing === item.kind" @click="install(item.kind)"><Download :size="12" />安装</BaseButton>
            <BaseButton v-else-if="item.managed" variant="danger" size="sm" :busy="uninstalling === item.kind" @click="uninstall(item.kind)"><Trash2 :size="12" />卸载</BaseButton>
            <span v-else class="text-[10px] text-[var(--text-muted)]">系统安装</span>
          </div>
          <p v-if="!loading && software.length === 0" class="col-span-full py-6 text-center text-xs text-[var(--text-muted)]">暂无软件信息。</p>
        </div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold">下载设置</h2><p class="mt-1 text-xs text-[var(--text-muted)]">用于下载安装受管的隧道客户端。</p></div>
        <div class="grid gap-3">
          <TextField v-model="config.githubMirror" label="GitHub 镜像" placeholder="https://gh-proxy.com" hint="留空则直连 GitHub。" />
          <SelectField v-model="config.proxyMode" label="代理模式" :options="proxyOptions" />
          <TextField v-if="config.proxyMode === 'manual'" v-model="config.proxyUrl" label="代理地址" placeholder="http://127.0.0.1:7890" />
          <div class="flex justify-end"><BaseButton :busy="saving" :disabled="!dirty" @click="saveConfig"><Save :size="14" />保存设置</BaseButton></div>
        </div>
      </GlassCard>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.software-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .5rem;
  min-width: 0;
}

@container app-main (max-width: 720px) {
  .settings-page {
    padding-inline: 16px;
  }

  .software-grid {
    grid-template-columns: 1fr;
  }
}
</style>
