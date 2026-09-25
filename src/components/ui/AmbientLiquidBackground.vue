<template>
  <div class="fixed inset-0 overflow-hidden pointer-events-none z-0 ambient-liquid-canvas" aria-hidden="true">
    <!-- Prompt-specified Concentric Circles Overlay (Top-Right) -->
    <svg
      class="absolute -top-[120px] -right-[120px] w-[620px] h-[620px] opacity-35 dark:opacity-25 will-change-transform pointer-events-none"
      viewBox="0 0 600 600"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style="animation: ambientConcentricFloat 45s ease-in-out infinite alternate;"
    >
      <!-- 主色描边圆环 r=260 -->
      <circle cx="300" cy="300" r="260" stroke-width="1.5" stroke-opacity="0.5" stroke-dasharray="3 5" style="stroke: var(--primary)" />
      <!-- 主色描边圆环 r=180 -->
      <circle cx="300" cy="300" r="180" stroke-width="1.5" stroke-opacity="0.7" style="stroke: var(--primary)" />
      <!-- 白色圆环 r=100 -->
      <circle cx="300" cy="300" r="100" stroke="#ffffff" stroke-width="1.5" stroke-opacity="0.65" />
      <!-- 主色圆点 r=26 -->
      <circle cx="300" cy="300" r="26" style="fill: var(--primary)" />
    </svg>

    <!-- Subtle Secondary Concentric Accent (Bottom-Left) -->
    <svg
      class="absolute -bottom-[160px] -left-[160px] w-[540px] h-[540px] opacity-25 dark:opacity-15 will-change-transform pointer-events-none"
      viewBox="0 0 600 600"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style="animation: ambientConcentricFloatRev 55s ease-in-out infinite alternate;"
    >
      <circle cx="300" cy="300" r="260" stroke-width="1.2" stroke-opacity="0.35" style="stroke: var(--primary)" />
      <circle cx="300" cy="300" r="180" stroke-width="1.2" stroke-opacity="0.5" stroke-dasharray="2 4" style="stroke: var(--primary)" />
      <circle cx="300" cy="300" r="100" stroke="#ffffff" stroke-width="1.2" stroke-opacity="0.4" />
      <circle cx="300" cy="300" r="26" fill-opacity="0.6" style="fill: var(--primary)" />
    </svg>

    <!-- 4 Animated Ambient Blobs（颜色跟随当前调色盘主色） -->
    <div
      class="absolute -top-[5%] -left-[5%] w-[680px] h-[680px] rounded-full blur-[80px] will-change-transform"
      style="
        background: radial-gradient(circle at 45% 45%, color-mix(in srgb, var(--primary) 20%, transparent) 0%, color-mix(in srgb, var(--primary) 13%, transparent) 45%, transparent 80%);
        animation: morphBlob1 24s infinite ease-in-out;
      "
    ></div>

    <div
      class="absolute top-[8%] -right-[8%] w-[780px] h-[780px] rounded-full blur-[90px] will-change-transform"
      style="
        background: radial-gradient(circle at 50% 50%, color-mix(in srgb, var(--primary) 17%, transparent) 0%, color-mix(in srgb, var(--primary) 11%, transparent) 45%, rgba(255, 255, 255, 0.08) 65%, transparent 80%);
        animation: morphBlob2 28s infinite ease-in-out;
      "
    ></div>

    <div
      class="absolute -bottom-[10%] left-[8%] w-[720px] h-[720px] rounded-full blur-[85px] will-change-transform"
      style="
        background: radial-gradient(circle at 55% 45%, color-mix(in srgb, var(--primary) 16%, transparent) 0%, color-mix(in srgb, var(--primary) 10%, transparent) 45%, transparent 80%);
        animation: morphBlob3 22s infinite ease-in-out;
      "
    ></div>

    <div
      class="absolute bottom-[2%] right-[8%] w-[640px] h-[640px] rounded-full blur-[75px] will-change-transform"
      style="
        background: radial-gradient(circle at 50% 50%, color-mix(in srgb, var(--primary) 15%, transparent) 0%, color-mix(in srgb, var(--primary) 9%, transparent) 45%, transparent 80%);
        animation: morphBlob4 26s infinite ease-in-out;
      "
    ></div>

    <!-- 3 Specular Caustic Highlights -->
    <div
      class="absolute top-[28%] left-[24%] w-[90px] h-[90px] rounded-full blur-[22px] will-change-transform"
      style="
        background: radial-gradient(circle, rgba(255, 255, 255, 0.85) 0%, color-mix(in srgb, var(--primary) 42%, transparent) 40%, color-mix(in srgb, var(--primary) 14%, transparent) 65%, transparent 85%);
        animation: causticFloat1 9s infinite ease-in-out;
      "
    ></div>

    <div
      class="absolute top-[58%] right-[28%] w-[120px] h-[120px] rounded-full blur-[28px] will-change-transform"
      style="
        background: radial-gradient(circle, rgba(255, 255, 255, 0.8) 0%, color-mix(in srgb, var(--primary) 38%, transparent) 45%, color-mix(in srgb, var(--primary) 12%, transparent) 70%, transparent 85%);
        animation: causticFloat2 11s infinite ease-in-out;
      "
    ></div>

    <div
      class="absolute bottom-[22%] left-[42%] w-[80px] h-[80px] rounded-full blur-[18px] will-change-transform"
      style="
        background: radial-gradient(circle, rgba(255, 255, 255, 0.8) 0%, color-mix(in srgb, var(--primary) 40%, transparent) 40%, color-mix(in srgb, var(--primary) 12%, transparent) 65%, transparent 85%);
        animation: causticFloat3 8s infinite ease-in-out;
      "
    ></div>

    <!-- Glass-refraction Layer -->
    <div
      class="absolute inset-0"
      style="
        background: linear-gradient(135deg, rgba(255, 255, 255, 0.015) 0%, rgba(255, 255, 255, 0.006) 45%, rgba(255, 255, 255, 0.002) 100%);
        backdrop-filter: blur(0.5px);
        -webkit-backdrop-filter: blur(0.5px);
      "
    ></div>

    <!-- Noise Texture Overlay -->
    <div
      class="absolute inset-0 opacity-[0.018] mix-blend-overlay"
      :style="{
        backgroundImage: `url(&quot;data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='ambientNoiseFilter'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.75' numOctaves='3' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23ambientNoiseFilter)'/%3E%3C/svg%3E&quot;)`,
      }"
    ></div>
  </div>
</template>

<script setup lang="ts">
// 环境氛围背景：所有强调色均跟随当前调色盘的 --primary（见 src/lib/palette.ts）
</script>

<style scoped>
.ambient-liquid-canvas {
  background-color: var(--ios-canvas-bg, var(--page-bg, #0e0b0a));
  transition: background-color 300ms ease;
}

@keyframes ambientConcentricFloat {
  0% {
    transform: translate3d(0, 0, 0) rotate(0deg) scale(1);
  }
  50% {
    transform: translate3d(-18px, 14px, 0) rotate(6deg) scale(1.03);
  }
  100% {
    transform: translate3d(12px, -10px, 0) rotate(-4deg) scale(0.98);
  }
}

@keyframes ambientConcentricFloatRev {
  0% {
    transform: translate3d(0, 0, 0) rotate(0deg) scale(1);
  }
  50% {
    transform: translate3d(16px, -12px, 0) rotate(-5deg) scale(1.02);
  }
  100% {
    transform: translate3d(-10px, 14px, 0) rotate(7deg) scale(0.97);
  }
}
</style>
