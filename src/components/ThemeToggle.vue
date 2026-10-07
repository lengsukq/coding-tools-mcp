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
    class="ui-icon-button"
    :aria-label="dark ? '切换到浅色模式' : '切换到深色模式'"
    :title="dark ? '切换到浅色模式' : '切换到深色模式'"
    @click="applyTheme(!dark)"
  >
    <Sun v-if="dark" :size="14" />
    <Moon v-else :size="14" />
  </button>
</template>
