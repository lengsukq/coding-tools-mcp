import { onBeforeUnmount, ref } from "vue";
import { showToast } from "$lib/stores/toast";

const DEFAULT_COPIED_RESET_MS = 1600;

export interface CopyTextOptions {
  key?: string;
  label?: string;
  durationMs?: number;
  silentSuccess?: boolean;
}

export function useClipboard(defaultDurationMs = DEFAULT_COPIED_RESET_MS) {
  const copiedKey = ref("");
  let resetTimer: ReturnType<typeof setTimeout> | undefined;

  function clearCopied() {
    if (resetTimer !== undefined) {
      clearTimeout(resetTimer);
      resetTimer = undefined;
    }
    copiedKey.value = "";
  }

  async function copyText(text: string, options: CopyTextOptions = {}): Promise<boolean> {
    if (!text) return false;
    const key = options.key ?? "default";
    try {
      await navigator.clipboard.writeText(text);
      copiedKey.value = key;
      if (resetTimer !== undefined) clearTimeout(resetTimer);
      resetTimer = setTimeout(() => {
        if (copiedKey.value === key) copiedKey.value = "";
        resetTimer = undefined;
      }, options.durationMs ?? defaultDurationMs);
      if (options.label && !options.silentSuccess) {
        showToast(`已复制${options.label}`, { kind: "success", duration: 2200 });
      }
      return true;
    } catch (error) {
      showToast(String(error), {
        title: options.label ? `复制${options.label}失败` : "复制失败",
        kind: "error",
      });
      return false;
    }
  }

  onBeforeUnmount(() => {
    if (resetTimer !== undefined) clearTimeout(resetTimer);
  });

  return {
    copiedKey,
    copyText,
    clearCopied,
  };
}
