<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import appIcon from "$src/assets/app-icon.png";
import BaseButton from "../../components/ui/BaseButton.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import SelectField from "../../components/ui/SelectField.vue";
import TextField from "../../components/ui/TextField.vue";
import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
import SaveBar from "../../components/ui/SaveBar.vue";
import PalettePicker from "../../components/settings/PalettePicker.vue";
import GeneralContextCard from "../../components/settings/GeneralContextCard.vue";
import GeneralPermissionsCard from "../../components/settings/GeneralPermissionsCard.vue";
import GeneralProxyCard from "../../components/settings/GeneralProxyCard.vue";
import { confirmUnsavedChanges, useUnsavedChanges } from "$lib/unsaved-changes";
import { SETTINGS_NAV } from "$lib/ui-model";
import { enqueueRuntimeSettingsWrite } from "$lib/queued-writes";
import { mergeOwnedChanges } from "$lib/runtime-settings-draft";
import { checkAppUpdate, openUrl } from "$lib/api/app-info";
import { scanGlobalAgentContext, type GlobalAgentContextScanDto } from "$lib/api/agent-context";
import {
  getGlobalRuntimeSettings,
  getProxy,
  setGlobalRuntimeSettings,
  setProxy,
  type GlobalRuntimeSettingsDto,
  type ProxyConfigDto,
} from "$lib/api/settings";
import { getWebviewMemorySample } from "$lib/api/ui-memory";
import { enterCompactMode } from "$lib/api/window-chrome";
import { APP_VERSION, RELEASES_LATEST_URL, REPO_URL } from "$lib/app-meta";
import { isMacPlatform } from "$lib/platform";
import { showToast } from "$lib/stores/toast";
import { reloadUiOnly } from "$lib/ui-memory-guard";
import { isValidProxyUrl } from "$lib/validators";

const props = withDefaults(
  defineProps<{
    section?: string;
    panel?: string;
  }>(),
  { section: "general", panel: "" },
);

const AUTH_OPTIONS = [
  { value: "oauth", label: "OAuth（推荐）" },
  { value: "bearer", label: "Bearer Token" },
  { value: "noauth", label: "无认证（仅可信本机环境）" },
];

function cloneRuntimeSettings(source: GlobalRuntimeSettingsDto): GlobalRuntimeSettingsDto {
  return {
    ...source,
    instructionSources: [...source.instructionSources],
    skillSources: [...source.skillSources],
    disabledInstructionIds: [...(source.disabledInstructionIds ?? [])],
    disabledSkillIds: [...(source.disabledSkillIds ?? [])],
  };
}

function isRuntimeFieldEqual(
  a: GlobalRuntimeSettingsDto[keyof GlobalRuntimeSettingsDto],
  b: GlobalRuntimeSettingsDto[keyof GlobalRuntimeSettingsDto],
): boolean {
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, index) => item === b[index]);
  }
  return a === b;
}

const proxy = reactive<ProxyConfigDto>({ mode: "none", url: "" });
const runtime = reactive<GlobalRuntimeSettingsDto>({
  authType: "oauth",
  mcpServerName: "",
  executablePaths: "",
  permissionMode: "trusted",
  allowedCommands: "",
  aiInstructions: "",
  instructionSources: [],
  skillSources: [],
  customInstructionPaths: "",
  customSkillPaths: "",
  disabledInstructionIds: [],
  disabledSkillIds: [],
  allowLanAccess: false,
  allowLocalNoauth: true,
  restoreRuntimeStateOnLaunch: false,
  launchAtLogin: false,
  launchInLightweightMode: false,
  migrationNotice: "",
});

const loading = ref(false);
const proxySaving = ref(false);
const runtimeSaving = ref(false);
const checkingUpdate = ref(false);
const releasingUi = ref(false);
const canCompact = isMacPlatform;
const memoryHint = ref<string | null>(null);
const scan = ref<GlobalAgentContextScanDto | null>(null);
const scanning = ref(false);
const loadError = ref("");
const runtimeError = ref("");
const proxyError = ref("");
const runtimeBaseline = ref<GlobalRuntimeSettingsDto | null>(null);
const proxyBaseline = ref<ProxyConfigDto | null>(null);

const needsRuntime = computed(() => ["general", "connection", "execution"].includes(props.section));
const needsProxy = computed(() => props.section === "tools");

const owned = computed<Array<keyof GlobalRuntimeSettingsDto>>(() => {
  if (props.section === "connection") {
    return ["authType", "mcpServerName", "allowLocalNoauth", "allowLanAccess"];
  }
  if (props.section === "general") {
    return ["launchAtLogin", "launchInLightweightMode", "restoreRuntimeStateOnLaunch"];
  }
  if (props.section === "execution") {
    if (props.panel === "context") {
      return [
        "aiInstructions",
        "instructionSources",
        "skillSources",
        "customInstructionPaths",
        "customSkillPaths",
        "disabledInstructionIds",
        "disabledSkillIds",
      ];
    }
    return ["permissionMode", "allowedCommands", "executablePaths"];
  }
  return [];
});

const runtimeDirty = computed(
  () =>
    !loading.value &&
    runtimeBaseline.value !== null &&
    owned.value.some((key) => !isRuntimeFieldEqual(runtime[key], runtimeBaseline.value![key])),
);

const proxyDirty = computed(
  () =>
    !loading.value &&
    needsProxy.value &&
    proxyBaseline.value !== null &&
    (proxy.mode !== proxyBaseline.value.mode || proxy.url !== proxyBaseline.value.url),
);

const activeUnsavedDirty = computed(() => (needsProxy.value ? proxyDirty.value : runtimeDirty.value));

function patchRuntime(patch: Partial<GlobalRuntimeSettingsDto>) {
  Object.assign(runtime, patch);
}

function patchProxy(patch: Partial<ProxyConfigDto>) {
  Object.assign(proxy, patch);
}

async function ensureSectionLoaded(force = false) {
  loadError.value = "";
  const loadRuntime = needsRuntime.value && (force || runtimeBaseline.value === null);
  const loadProxy = needsProxy.value && (force || proxyBaseline.value === null);

  if (loadRuntime || loadProxy) {
    loading.value = true;
    try {
      const [nextProxy, nextRuntime] = await Promise.all([
        loadProxy ? getProxy() : Promise.resolve(null),
        loadRuntime ? getGlobalRuntimeSettings() : Promise.resolve(null),
      ]);
      if (nextProxy) {
        Object.assign(proxy, nextProxy, { mode: nextProxy.mode === "custom" ? "manual" : nextProxy.mode });
        proxyBaseline.value = { ...proxy };
      }
      if (nextRuntime) {
        Object.assign(runtime, cloneRuntimeSettings(nextRuntime));
        runtimeBaseline.value = cloneRuntimeSettings(runtime);
      }
    } catch (error) {
      loadError.value = String(error);
    } finally {
      loading.value = false;
    }
  }

  if (props.section === "execution" && props.panel === "context" && (!scan.value || force)) {
    void scanSources();
  }
  if (props.section === "about" && (!memoryHint.value || force)) {
    void refreshMemory();
  }
}

watch(
  () => [props.section, props.panel],
  () => {
    void ensureSectionLoaded(false);
  },
);

async function compactUi() {
  if (!await confirmUnsavedChanges()) return;
  try {
    await enterCompactMode();
  } catch (error) {
    showToast(String(error), { title: "进入轻量模式失败", kind: "error" });
  }
}

async function scanSources() {
  scanning.value = true;
  try {
    scan.value = await scanGlobalAgentContext({
      customInstructionPaths: runtime.customInstructionPaths,
      customSkillPaths: runtime.customSkillPaths,
    });
  } catch (error) {
    showToast(String(error), { title: "扫描本机 Skills 与 Agents 失败", kind: "warning" });
  } finally {
    scanning.value = false;
  }
}

async function saveProxy() {
  if (proxySaving.value) return false;
  if (!proxyDirty.value) return true;
  proxySaving.value = true;
  proxyError.value = "";
  try {
    if (proxy.mode === "manual" && !isValidProxyUrl(proxy.url)) {
      throw new Error("代理地址须以 http://、https:// 或 socks5:// 开头。");
    }
    await setProxy({ ...proxy });
    proxyBaseline.value = { ...proxy };
    showToast("代理设置已保存", { kind: "success" });
    return true;
  } catch (e) {
    proxyError.value = String(e);
    return false;
  } finally {
    proxySaving.value = false;
  }
}

async function saveRuntime() {
  if (runtimeSaving.value) return false;
  if (!runtimeDirty.value || !runtimeBaseline.value) return true;
  runtimeSaving.value = true;
  runtimeError.value = "";
  try {
    const draft = cloneRuntimeSettings(runtime);
    const baseline = runtimeBaseline.value;
    const fields = [...owned.value];
    const customPathsChanged =
      props.section === "execution" &&
      props.panel === "context" &&
      (draft.customInstructionPaths !== baseline.customInstructionPaths ||
        draft.customSkillPaths !== baseline.customSkillPaths);
    await enqueueRuntimeSettingsWrite(async () => {
      const latest = await getGlobalRuntimeSettings();
      await setGlobalRuntimeSettings(mergeOwnedChanges(latest, draft, baseline, fields));
    });
    runtimeBaseline.value = draft;
    if (customPathsChanged) {
      void scanSources();
    }
    showToast("设置已保存并自动应用（运行中的 MCP 已热更新生效）。", { kind: "success" });
    return true;
  } catch (e) {
    runtimeError.value = String(e);
    return false;
  } finally {
    runtimeSaving.value = false;
  }
}

function saveActiveSection() {
  return needsProxy.value ? saveProxy() : saveRuntime();
}

useUnsavedChanges(
  "应用设置 · " + (SETTINGS_NAV.find((item) => item.value === props.section)?.label ?? "通用"),
  activeUnsavedDirty,
  saveActiveSection,
);

async function checkUpdate() {
  checkingUpdate.value = true;
  try {
    const result = await checkAppUpdate();
    if (!result.updateAvailable) {
      showToast(`当前已是最新版本 v${result.currentVersion}`, { kind: "success" });
      return;
    }
    const openRelease = await ask(`发现新版本 ${result.latestTag}，是否打开 Releases？`, {
      title: "有可用更新",
      kind: "info",
      okLabel: "打开下载页",
      cancelLabel: "稍后",
    });
    if (openRelease) await openUrl(result.releaseUrl || RELEASES_LATEST_URL);
  } catch (error) {
    showToast(String(error), { title: "检查更新失败", kind: "error" });
  } finally {
    checkingUpdate.value = false;
  }
}

async function refreshMemory() {
  try {
    const sample = await getWebviewMemorySample();
    memoryHint.value = sample.supported
      ? `界面约 ${Math.round(sample.webviewMb)} MB（${sample.webviewProcessCount} 个 WebView），主进程约 ${Math.round(sample.mainMb)} MB。`
      : "当前平台暂不支持界面内存采样。";
  } catch (error) {
    console.warn("[GeneralSettingsView] failed to sample webview memory:", error);
    memoryHint.value = null;
  }
}

async function releaseUi() {
  const confirmed = await ask("将重建 WebView 释放界面内存，MCP 与隧道服务不会停止。", {
    title: "释放界面内存",
    kind: "info",
    okLabel: "立即释放",
    cancelLabel: "取消",
  });
  if (!confirmed) return;
  releasingUi.value = true;
  try {
    await reloadUiOnly("settings-manual");
  } finally {
    releasingUi.value = false;
  }
}

onMounted(() => {
  void ensureSectionLoaded(false);
});
</script>

<template>
  <div class="ui-stack">
    <p v-if="loading" role="status" class="ui-muted">正在加载设置…</p>
    <div v-else-if="loadError" class="ui-banner ui-banner--danger" role="alert">
      <span>设置加载失败：{{ loadError }}</span>
      <BaseButton variant="secondary" @click="ensureSectionLoaded(true)">重试</BaseButton>
    </div>

    <fieldset v-else :disabled="runtimeSaving || proxySaving" class="ui-stack">
      <!-- 外观与启动 -->
      <template v-if="section === 'general'">
        <GlassCard>
          <div class="ui-section-heading">
            <div>
              <h2>外观主题</h2>
              <p>品牌强调色独立于状态颜色，浅色与深色界面保持一致。</p>
            </div>
          </div>
          <PalettePicker />
        </GlassCard>

        <GlassCard>
          <div class="ui-section-heading">
            <div>
              <h2>启动行为</h2>
              <p>配置随系统开机自动启动、开机轻量驻留以及共享 MCP 服务的启动恢复策略。</p>
            </div>
          </div>
          <div class="ui-stack">
            <ToggleSwitch
              v-model="runtime.launchAtLogin"
              label="开机自动启动"
              :description="
                canCompact
                  ? '登录 macOS 时自动拉起本应用（通过用户 LaunchAgent 配置，无需管理员权限）。'
                  : '登录系统时自动启动本应用（仅作用于当前系统用户）。'
              "
            />
            <div class="ui-inset space-y-2 p-3.5">
              <ToggleSwitch
                v-model="runtime.launchInLightweightMode"
                :disabled="!runtime.launchAtLogin"
                label="开机时轻量启动"
                :description="
                  canCompact
                    ? '仅随开机自启生效：启动时不创建主界面 WebView，仅驻留顶部菜单栏以节省内存；随时可从菜单栏恢复主窗口。'
                    : '仅随开机自启生效：启动时不弹出主窗口，静默驻留系统托盘；点击托盘图标即可打开界面。'
                "
              />
              <p
                v-if="runtime.launchAtLogin && runtime.launchInLightweightMode && !runtime.restoreRuntimeStateOnLaunch"
                class="text-xs leading-5 text-amber-700 dark:text-amber-300"
              >
                提示：若希望开机轻量启动后 MCP 服务自动就绪，建议同时开启下方「启动时恢复 MCP 服务」。
              </p>
            </div>
            <ToggleSwitch
              v-model="runtime.restoreRuntimeStateOnLaunch"
              label="启动时恢复 MCP 服务"
              description="影响应用下次启动（包含开机轻量启动）；所有工作区共享同一个 MCP 服务。"
            />
          </div>
          <SaveBar :dirty="runtimeDirty" :busy="runtimeSaving" :error="runtimeError" @save="saveRuntime" />
        </GlassCard>
      </template>

      <!-- 服务名称、认证与访问范围 -->
      <GlassCard v-else-if="section === 'connection'" id="settings-auth-scope">
        <div class="ui-section-heading">
          <div>
            <h2>服务名称、认证与访问范围</h2>
            <p>影响全部工作区的连接；保存后自动热更新生效，无需手动重启 MCP。</p>
          </div>
        </div>
        <div class="ui-stack">
          <TextField
            v-model="runtime.mcpServerName"
            label="MCP 服务名称"
            placeholder="Coding Tools MCP（留空使用默认名称）"
            hint="用于 Gemini 等客户端展示与区分不同设备，例如：Coding Tools MCP · Mac mini；保存后对新连接立即生效。"
          />
          <SelectField v-model="runtime.authType" label="认证模式" :options="AUTH_OPTIONS" />
          <ToggleSwitch
            v-model="runtime.allowLocalNoauth"
            label="本机 Agent 免授权直连（推荐开启）"
            description="允许本机 127.0.0.1 非浏览器 Agent 免 OAuth 直接连接并访问已配对的局域网/组网工作区；公网隧道与跨机请求仍强制校验认证。"
          />
          <ToggleSwitch
            v-model="runtime.allowLanAccess"
            label="允许局域网未配对客户端直连 /mcp"
            description="开启局域网设备配对无需勾选此项；仅当需要其他电脑不经配对直接连接本机 /mcp 时启用。"
          />
          <p v-if="runtime.migrationNotice" class="ui-banner ui-banner--warning">{{ runtime.migrationNotice }}</p>
          <SaveBar :dirty="runtimeDirty" :busy="runtimeSaving" :error="runtimeError" @save="saveRuntime" />
        </div>
      </GlassCard>

      <!-- AI 执行：默认权限与白名单 / AI 指令与技能来源 -->
      <template v-else-if="section === 'execution'">
        <GeneralContextCard
          v-if="panel === 'context'"
          :runtime="runtime"
          :scan="scan"
          :scanning="scanning"
          :dirty="runtimeDirty"
          :saving="runtimeSaving"
          :error="runtimeError"
          @update-runtime="patchRuntime"
          @scan="scanSources()"
          @save="saveRuntime"
        />

        <GeneralPermissionsCard
          v-else
          :runtime="runtime"
          :dirty="runtimeDirty"
          :saving="runtimeSaving"
          :error="runtimeError"
          @update-runtime="patchRuntime"
          @save="saveRuntime"
        />
      </template>

      <!-- 网络代理 -->
      <GeneralProxyCard
        v-else-if="section === 'tools'"
        :proxy="proxy"
        :dirty="proxyDirty"
        :saving="proxySaving"
        :error="proxyError"
        @update-proxy="patchProxy"
        @save="saveProxy"
      />

      <!-- 关于与系统维护 -->
      <div v-else-if="section === 'about'" class="ui-bento-grid ui-bento-grid--compact items-stretch">
        <GlassCard>
          <div class="ui-section-heading">
            <div>
              <h2>Coding Tools MCP</h2>
              <p>本地 AI 开发工作台 · v{{ APP_VERSION }}</p>
            </div>
            <img :src="appIcon" alt="" class="size-11 rounded-xl" />
          </div>
          <p class="ui-secondary text-sm leading-6">统一编排多工作区上下文、跨设备联邦路由、AI 修改快照对比与任务验收门禁。</p>
          <div class="ui-actions mt-5">
            <BaseButton variant="secondary" :busy="checkingUpdate" @click="checkUpdate">检查更新</BaseButton>
            <BaseButton variant="ghost" @click="openUrl(REPO_URL)">仓库与帮助</BaseButton>
          </div>
        </GlassCard>

        <GlassCard>
          <div class="ui-section-heading">
            <div>
              <h2>界面内存与后台驻留</h2>
              <p>回收 WebView 渲染进程内存不会中断正在运行的 MCP 服务与隧道。</p>
            </div>
          </div>
          <div class="ui-inset p-3.5 text-xs text-[var(--ui-text-secondary)]">
            {{ memoryHint || "内存信息暂不可用。" }}
          </div>
          <div class="ui-actions mt-4">
            <BaseButton variant="secondary" size="sm" @click="refreshMemory">刷新采样</BaseButton>
            <BaseButton variant="ghost" size="sm" :busy="releasingUi" @click="releaseUi">释放界面内存</BaseButton>
            <BaseButton v-if="canCompact" variant="secondary" size="sm" @click="compactUi">轻量驻留（关闭界面）</BaseButton>
          </div>
        </GlassCard>
      </div>
    </fieldset>
  </div>
</template>
