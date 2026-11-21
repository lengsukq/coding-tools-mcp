<script setup lang="ts">
import { computed } from "vue";
import { Folder, GitBranch } from "@lucide/vue";
import type { WorkspaceProfile } from "$lib/types";
import { peekCachedPlanningState, peekCachedWorkspaceGitSummary } from "$lib/workspace-status-cache";
import { statusPresentation, taskFocus } from "$lib/ui-model";

const props = defineProps<{
  workspace: WorkspaceProfile;
  active?: boolean;
}>();
defineEmits<{ click: [] }>();

// The sidebar is always mounted. Never start one Git/Planning poller per item.
// It shows last known data and lets the active page refresh on demand.
const planning = computed(() => peekCachedPlanningState(props.workspace.id));
const git = computed(() => peekCachedWorkspaceGitSummary(props.workspace.id));
const focus = computed(() => taskFocus(planning.value));
const statePresentation = computed(() =>
  planning.value ? statusPresentation(planning.value.execution.state) : { label: "打开后加载状态", tone: "neutral" },
);
const dotColor = computed(() => {
  if (!planning.value) return "bg-[var(--ui-line-strong)]";
  if (statePresentation.value.tone === "success") return "bg-[var(--ui-success)]";
  if (statePresentation.value.tone === "warning") return "bg-[var(--ui-warning)]";
  if (statePresentation.value.tone === "danger") return "bg-[var(--ui-danger)]";
  if (statePresentation.value.tone === "info") return "bg-[var(--ui-accent)]";
  return "bg-[var(--ui-text-muted)]/60";
});
const gitMeta = computed(() => {
  if (!git.value) return null;
  if (git.value.available) {
    const branch = git.value.branch || "HEAD";
    return git.value.changedFiles > 0 ? `${branch} · ${git.value.changedFiles}` : branch;
  }
  if (git.value.subRepositories.length) return `${git.value.subRepositories.length} 子仓库`;
  return null;
});
const subtitle = computed(() => {
  if (focus.value.plan || focus.value.goal) return focus.value.title;
  if (gitMeta.value) return gitMeta.value;
  return props.workspace.path.split(/[\\/]/).filter(Boolean).slice(-2).join("/");
});
</script>

<template>
  <button
    type="button"
    class="ui-workspace-nav group"
    :class="{ active }"
    :aria-current="active ? 'page' : undefined"
    :title="`${workspace.path}\n${focus.title}\n${statePresentation.label}`"
    @click="$emit('click')"
  >
    <div
      class="mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-[11px] border transition-colors"
      :class="
        active
          ? 'border-[var(--ui-accent)]/25 bg-[var(--ui-accent)]/12 text-[var(--ui-accent)]'
          : 'border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)] group-hover:text-[var(--ui-text)]'
      "
    >
      <Folder :size="15" class="!mt-0" />
    </div>
    <span class="min-w-0 flex-1">
      <span class="flex items-center justify-between gap-2">
        <strong class="truncate">{{ workspace.name }}</strong>
        <span class="h-2 w-2 shrink-0 rounded-full" :class="dotColor" :title="statePresentation.label" />
      </span>
      <small class="truncate">{{ subtitle }}</small>
      <small v-if="gitMeta && (focus.plan || focus.goal)" class="mt-1 flex max-w-full min-w-0 items-center gap-1 text-[11px] opacity-85">
        <GitBranch :size="11" class="!mt-0 shrink-0" />
        <span class="min-w-0 flex-1 truncate">{{ gitMeta }}</span>
      </small>
    </span>
  </button>
</template>
