<script setup lang="ts">
import { AlertCircle, X } from "@lucide/vue";
import ModalDialog from "./ui/ModalDialog.vue";
import BaseButton from "./ui/BaseButton.vue";
import { ref } from "vue";
import { hideToTray, quitApp } from "$lib/api/window-chrome";
import { showToast } from "$lib/stores/toast";

const open = defineModel<boolean>("open", { default: false });
const busy = ref(false);

async function run(action: "hide" | "quit") {
  if (busy.value) return;
  busy.value = true;
  try {
    if (action === "hide") await hideToTray();
    else await quitApp();
    open.value = false;
  } catch (error) {
    showToast(String(error), { title: "操作失败", kind: "error" });
  } finally {
    busy.value = false;
  }
}
</script>

<template><ModalDialog :open="open" title="关闭 Coding Tools MCP？" description="隐藏到托盘可保持 MCP 服务，退出会关闭应用。" :dismissible="!busy" @close="open=false"><div class="ui-actions justify-end"><BaseButton variant="ghost" :disabled="busy" @click="open=false">取消</BaseButton><BaseButton variant="secondary" :busy="busy" @click="run('hide')">隐藏到托盘</BaseButton><BaseButton :busy="busy" @click="run('quit')">退出应用</BaseButton></div></ModalDialog></template>
