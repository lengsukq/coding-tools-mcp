<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { Check, ChevronDown, Copy, History } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import { showToast } from "$lib/stores/toast";

const sessionPrompt = `不需要强制调用 history_session_bootstrap，直接处理当前任务即可。
如果需要以前的工作记录，先调用 history_session_search，再用 history_session_read 分页读取相关原始 Markdown；不要把全部历史记录加载到上下文。
如果当前工作区启用了会话记录，在每轮任务完成后调用 history_session_checkpoint，并传入本轮请求的 raw_user_input；session_key 和 expected_path 可以省略，服务端会懒初始化当前会话。
只有 checkpoint 返回 recorded=true 或 ok=true 后，才能确认进度已保存；服务端不能自动读取未通过工具参数传入的对话内容。历史上下文也可以在 Coding Tools MCP 面板中多选后注入。`;

const copying = ref(false);
const copied = ref(false);
const expanded = ref(false);
const errorMessage = ref("");
let resetTimer: ReturnType<typeof setTimeout> | undefined;

async function copyPrompt() {
  if (copying.value) return;
  copying.value = true;
  copied.value = false;
  errorMessage.value = "";
  if (resetTimer) clearTimeout(resetTimer);
  try {
    await navigator.clipboard.writeText(sessionPrompt);
    copied.value = true;
    showToast("新会话启动提示词已复制，可以直接粘贴到 ChatGPT。", { title: "复制成功", kind: "success", duration: 2500 });
    resetTimer = setTimeout(() => { copied.value = false; }, 2000);
  } catch (error) {
    errorMessage.value = "复制失败，请选中提示词后手动复制。";
    showToast(String(error), { title: "无法复制提示词", kind: "error", duration: 6000 });
  } finally {
    copying.value = false;
  }
}

onBeforeUnmount(() => {
  if (resetTimer) clearTimeout(resetTimer);
});
</script>

<template>
  <section class="ios-glass ios-card-surface p-4" aria-labelledby="chatgpt-session-prompt-title">
    <div class="flex items-center justify-between gap-4">
      <div class="flex min-w-0 items-center gap-3">
        <span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-[var(--primary-soft)] text-[var(--primary)]"><History :size="17" /></span>
        <div class="min-w-0">
          <h3 id="chatgpt-session-prompt-title" class="text-sm font-semibold">ChatGPT 新会话启动提示词</h3>
          <p class="mt-0.5 text-xs leading-5 text-[var(--text-muted)]">当前会话默认记录；旧会话由面板选择后按需注入。</p>
        </div>
      </div>
      <div class="flex shrink-0 gap-2">
        <BaseButton :busy="copying" @click="copyPrompt"><Check v-if="copied" :size="14" /><Copy v-else :size="14" />{{ copied ? '已复制' : '复制完整提示词' }}</BaseButton>
        <BaseButton variant="secondary" @click="expanded = !expanded">{{ expanded ? '收起提示词' : '查看完整提示词' }}<ChevronDown :size="14" class="transition-transform" :class="expanded ? 'rotate-180' : ''" /></BaseButton>
      </div>
    </div>
    <Transition name="prompt-expand">
      <div v-if="expanded" id="chatgpt-session-prompt-content" class="mt-4 border-t border-black/[.055] pt-4 dark:border-white/[.07]">
        <pre class="whitespace-pre-wrap break-words rounded-2xl bg-black/[.035] p-3 font-mono text-[11px] leading-5 text-[var(--text-secondary)] dark:bg-white/[.045]">{{ sessionPrompt }}</pre>
        <p class="mt-2 text-[11px] leading-5 text-[var(--text-muted)]">复制后粘贴到使用当前工作区 MCP 连接器的 ChatGPT 新会话。</p>
      </div>
    </Transition>
    <p v-if="errorMessage" class="mt-2 text-xs text-[#ff375f]" role="alert">{{ errorMessage }}</p>
    <span class="sr-only" aria-live="polite">{{ copied ? '提示词已复制' : '' }}</span>
  </section>
</template>

<style scoped>
.prompt-expand-enter-active,.prompt-expand-leave-active{transition:opacity .18s ease,transform .18s ease}.prompt-expand-enter-from,.prompt-expand-leave-to{opacity:0;transform:translateY(-4px)}
</style>
