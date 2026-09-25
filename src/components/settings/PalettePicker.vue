<script setup lang="ts">
import { ref } from "vue";
import { Check } from "@lucide/vue";
import { PALETTES, applyPalette, getSavedPaletteId } from "$lib/palette";

const selected = ref<string>(getSavedPaletteId());

function pick(id: string) {
  selected.value = id;
  applyPalette(id);
}
</script>

<template>
  <div class="palette-grid" role="radiogroup" aria-label="主题调色盘">
    <button
      v-for="p in PALETTES"
      :key="p.id"
      type="button"
      role="radio"
      :aria-checked="selected === p.id"
      class="palette-card"
      :class="{ 'is-active': selected === p.id }"
      :title="p.desc"
      @click="pick(p.id)"
    >
      <span
        class="palette-swatch"
        :style="{ background: `linear-gradient(135deg, ${p.preview[0]} 0%, ${p.preview[1]} 55%, ${p.preview[2]} 100%)` }"
      >
        <Check v-if="selected === p.id" :size="16" class="palette-check" />
      </span>
      <span class="palette-meta">
        <span class="palette-name">{{ p.name }}</span>
        <span class="palette-desc">{{ p.desc }}</span>
      </span>
    </button>
  </div>
</template>

<style scoped>
.palette-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
  gap: 10px;
}

.palette-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 16px;
  border: 1px solid var(--border-light);
  background: var(--card-bg);
  cursor: pointer;
  text-align: left;
  transition: transform var(--duration-fast, 0.15s) var(--ease-out, ease),
    border-color var(--duration-fast, 0.15s) ease,
    box-shadow var(--duration-fast, 0.15s) ease;
}

.palette-card:hover {
  transform: translateY(-1px);
  border-color: var(--border);
  box-shadow: var(--shadow-sm);
}

.palette-card.is-active {
  border-color: var(--primary);
  box-shadow: var(--focus-ring);
}

.palette-swatch {
  position: relative;
  flex: 0 0 34px;
  width: 34px;
  height: 34px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35), 0 2px 6px rgba(0, 0, 0, 0.12);
}

.palette-check {
  color: #fff;
  filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.4));
}

.palette-meta {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.palette-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-main);
  line-height: 1.3;
}

.palette-desc {
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
