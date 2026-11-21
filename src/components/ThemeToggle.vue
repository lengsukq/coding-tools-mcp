<script setup lang="ts">
import { Moon, Sun } from "@lucide/vue";
import { onMounted, onUnmounted, ref } from "vue";

const dark = ref(false);

function applyTheme(nextDark: boolean) {
  dark.value = nextDark;
  const theme = nextDark ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  document.documentElement.classList.toggle("dark", nextDark);
  try {
    localStorage.setItem("coding-tools-theme", theme);
  } catch (error) {
    console.warn("[ThemeToggle] failed to persist theme:", error);
  }
}

function handleToggleEvent() {
  applyTheme(!dark.value);
}

onMounted(() => {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem("coding-tools-theme");
  } catch (error) {
    console.warn("[ThemeToggle] failed to read saved theme:", error);
  }
  const initialDark = saved !== "light";
  applyTheme(initialDark);
  window.addEventListener("coding-tools:toggle-theme", handleToggleEvent);
});

onUnmounted(() => {
  window.removeEventListener("coding-tools:toggle-theme", handleToggleEvent);
});
</script>

<template>
  <button
    type="button"
    class="ui-icon-button"
    :aria-label="dark ? '切换到浅色模式' : '切换到深色模式'"
    :title="dark ? '切换到浅色模式' : '切换到深色模式'"
    @click="applyTheme(!dark)"
  >
    <Sun v-if="dark" :size="14" />
    <Moon v-else :size="14" />
  </button>
</template>
