<script setup lang="ts">
import { AlertCircle, X } from "@lucide/vue";
import ModalDialog from "./ui/ModalDialog.vue";
import BaseButton from "./ui/BaseButton.vue";
import { ref } from "vue";
import { enterCompactMode, hideToTray, quitApp } from "$lib/api/window-chrome";
import { showToast } from "$lib/stores/toast";

const open = defineModel<boolean>("open", { default: false });
const busy = ref(false);
const canCompact = /Macintosh|Mac OS X/.test(navigator.userAgent);

async function run(action: "hide" | "compact" | "quit") {
  if (busy.value) return;
  busy.value = true;
  try {
    if (action === "hide") await hideToTray();
    else if (action === "compact") await enterCompactMode();
    else await quitApp();
    open.value = false;
  } catch (error) {
    showToast(String(error), { title: "操作失败", kind: "error" });
  } finally {
    busy.value = false;
  }
}
</script>

<template><ModalDialog :open="open" title="关闭 Coding Tools MCP？" :description="canCompact ? '轻量驻留会释放主界面内存，MCP 服务保持运行；通过菜单栏图标恢复。' : '隐藏到托盘可保持 MCP 服务，退出会关闭应用。'" :dismissible="!busy" @close="open=false"><div class="ui-actions justify-end flex-wrap"><BaseButton variant="ghost" :disabled="busy" @click="open=false">取消</BaseButton><BaseButton variant="secondary" :busy="busy" @click="run('hide')">仅隐藏</BaseButton><BaseButton v-if="canCompact" variant="secondary" :busy="busy" @click="run('compact')">轻量驻留</BaseButton><BaseButton :busy="busy" @click="run('quit')">退出应用</BaseButton></div></ModalDialog></template>
