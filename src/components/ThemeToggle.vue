<script setup lang="ts">
import { Moon, Sun } from "@lucide/vue";
import { onMounted, ref } from "vue";

const dark = ref(false);

function applyTheme(nextDark: boolean) {
  dark.value = nextDark;
  const theme = nextDark ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  document.documentElement.classList.toggle("dark", nextDark);
  localStorage.setItem("coding-tools-theme", theme);
}

onMounted(() => {
  const saved = localStorage.getItem("coding-tools-theme");
  const initialDark = saved === "light" ? false : true;
  applyTheme(initialDark);
});
</script>

<template>
  <button
    type="button"
    class="ios-theme-toggle"
    :title="dark ? '切换到浅色模式' : '切换到深色模式'"
    @click="applyTheme(!dark)"
  >
    <Sun v-if="dark" :size="14" />
    <Moon v-else :size="14" />
  </button>
</template>

<style scoped>
.ios-theme-toggle {
  display: grid;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  place-items: center;
  border: 1px solid rgba(255,255,255,.72);
  border-radius: 12px;
  background: rgba(255,255,255,.66);
  box-shadow: inset 0 1px 0 rgba(255,255,255,.76), 0 5px 14px rgba(var(--pal-rgb,190,120,110),.1);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 150ms ease, color 150ms ease, transform 150ms ease;
}
.ios-theme-toggle:hover { background: rgba(255,255,255,.95); color: var(--primary); }
.ios-theme-toggle:active { transform: scale(.95); }
:global(.dark) .ios-theme-toggle {
  border-color: rgba(255,255,255,.07);
  background: rgba(255,255,255,.055);
  box-shadow: inset 0 1px 0 rgba(255,255,255,.04);
}
</style>
