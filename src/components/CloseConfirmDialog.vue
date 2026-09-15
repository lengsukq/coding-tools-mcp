<script setup lang="ts">
import { AlertCircle, X } from "@lucide/vue";
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

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="open" class="fixed inset-0 z-[1100] grid place-items-center bg-black/25 p-6 backdrop-blur-sm" @click.self="open = false">
        <div class="ios-glass-strong ios-floating-surface w-full max-w-md p-5">
          <div class="flex items-start gap-3">
            <div class="grid h-10 w-10 place-items-center rounded-2xl bg-[rgba(255,159,10,.14)] text-[var(--warning)]">
              <AlertCircle :size="20" />
            </div>
            <div class="min-w-0 flex-1">
              <h3 class="text-base font-semibold text-[var(--text-main)]">关闭 Coding Tools MCP？</h3>
              <p class="mt-1 text-xs leading-5 text-[var(--text-secondary)]">你可以隐藏到托盘继续保持 Runtime，或完全退出应用。</p>
            </div>
            <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="open = false"><X :size="13" /></button>
          </div>
          <div class="mt-5 flex justify-end gap-2">
            <button class="tx-btn tx-btn-ghost" type="button" :disabled="busy" @click="open = false">取消</button>
            <button class="tx-btn tx-btn-secondary" type="button" :disabled="busy" @click="run('hide')">隐藏到托盘</button>
            <button class="tx-btn tx-btn-primary" type="button" :disabled="busy" @click="run('quit')">退出应用</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-enter-active,
.modal-leave-active { transition: opacity 160ms ease; }
.modal-enter-from,
.modal-leave-to { opacity: 0; }
</style>
