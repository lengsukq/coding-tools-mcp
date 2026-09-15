# 技术配置

当前桌面端使用 **Vue 3 + Vite 6 + Vue Router 4 + Tailwind CSS 4**。Tailwind 通过 `@tailwindcss/vite` 直接接入，项目不维护旧式 `tailwind.config.js`；全局 Token 位于 `src/app.css` 与 `src/styles/ios-vue.css`。

## Vite

```js
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  build: { outDir: "build" },
});
```

Tauri 的生产目录保持为 `build/`。Router 使用 `createWebHashHistory()`，避免桌面端静态资源模式下动态路由依赖服务器 fallback。

## Tailwind CSS 4

`src/app.css` 必须保留：

```css
@import "tailwindcss";
```

组件优先使用 Tailwind utility；跨页面重复的视觉语义使用 CSS Token / utility class 收口，不在页面里重复大量 style 属性。

## iOS 彩色玻璃 Token

核心视觉变量定义于 `src/styles/ios-vue.css`：

```css
:root {
  --ios-blue: #0a84ff;
  --ios-indigo: #5e5ce6;
  --ios-purple: #bf5af2;
  --ios-pink: #ff375f;
  --ios-orange: #ff9f0a;
  --ios-green: #30d158;
  --ios-cyan: #64d2ff;
  --ios-glass: rgba(255, 255, 255, 0.68);
  --ios-radius: 22px;
}

.ios-glass {
  background: var(--ios-glass);
  backdrop-filter: blur(28px) saturate(180%);
  border: 1px solid var(--ios-glass-border);
  box-shadow: var(--ios-shadow);
}
```

设计原则：**彩色但不高饱和铺满、玻璃但保留 Card 层级、无界但不混淆信息分组**。Dashboard 与 Workspace 的主要信息仍使用明确卡片容器。

## Vue 组件示例

```vue
<script setup lang="ts">
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";

defineProps<{ name: string; state: string; path: string }>();
</script>

<template>
  <GlassCard :interactive="true">
    <div class="flex items-center gap-2">
      <h3 class="font-semibold">{{ name }}</h3>
      <StatusPill :status="state" />
    </div>
    <p class="mt-2 truncate font-mono text-xs text-[var(--text-muted)]">{{ path }}</p>
  </GlassCard>
</template>
```

基础组件集中在 `src/components/ui/`，页面优先组合这些组件，而不是自行复制按钮、输入框、Modal、Toggle 和状态标签样式。

## 图标

使用 `@lucide/vue`：

```ts
import { FolderOpen, Play, Square, Settings } from "@lucide/vue";
```

## 主题

主题由根节点 `data-theme="light|dark"` 驱动，`ThemeToggle.vue` 负责读取系统偏好并持久化用户选择。玻璃透明度、阴影与背景渐变必须同时提供 light / dark Token。

---
*返回: [README.md](./README.md)*
