<script lang="ts">
  import { Moon, Sun } from "@lucide/svelte";
  import { onMount } from "svelte";

  let dark = $state(true);

  onMount(() => {
    const stored = localStorage.getItem("theme");
    if (stored === "light" || stored === "dark") {
      dark = stored === "dark";
    } else {
      dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
    apply();
  });

  function apply() {
    const theme = dark ? "dark" : "light";
    document.documentElement.setAttribute("data-theme", theme);
    document.documentElement.classList.toggle("dark", dark);
    localStorage.setItem("theme", theme);
  }

  function toggle() {
    dark = !dark;
    apply();
  }
</script>

<button
  type="button"
  class="relative size-8 rounded-lg flex items-center justify-center text-[var(--sidebar-text-muted)] hover:text-[var(--sidebar-text)] hover:bg-[var(--sidebar-surface-hover)] active:scale-[0.92] transition-all cursor-pointer"
  onclick={toggle}
  title={dark ? "切换至浅色模式" : "切换至深色模式"}
  aria-label="切换主题"
>
  <span
    class="transition-transform duration-300 ease-out {dark ? 'rotate-0 scale-100' : 'rotate-90 scale-0 absolute'}"
  >
    <Sun size={15} strokeWidth={2.2} />
  </span>
  <span
    class="transition-transform duration-300 ease-out {!dark ? 'rotate-0 scale-100' : '-rotate-90 scale-0 absolute'}"
  >
    <Moon size={15} strokeWidth={2.2} />
  </span>
</button>
