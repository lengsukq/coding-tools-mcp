<script setup lang="ts">
import { computed } from "vue";
import { Folder } from "@lucide/vue";
import type { WorkspaceProfile } from "$lib/types";
import { peekCachedPlanningState, peekCachedWorkspaceGitSummary, statusRevision } from "$lib/workspace-status-cache";
import { taskFocus, statusPresentation } from "$lib/ui-model";
const props = defineProps<{
    workspace: WorkspaceProfile;
    active?: boolean;
}>();
defineEmits<{
    click: [
    ];
}>();
// The sidebar is always mounted. Never start one Git/Planning poller per item.
// It shows last known data and lets the active page refresh on demand.
const planning = computed(() => { statusRevision.value; return peekCachedPlanningState(props.workspace.id); });
const git = computed(() => { statusRevision.value; return peekCachedWorkspaceGitSummary(props.workspace.id); });
const focus = computed(() => taskFocus(planning.value));
const state = computed(() => planning.value ? statusPresentation(planning.value.execution.state).label : '打开后加载状态');
const meta = computed(() => !git.value ? '打开后加载 Git 状态' : git.value.available ? `${git.value.branch || '分离 HEAD'} · ${git.value.changedFiles} 个变更` : git.value.subRepositories.length ? `${git.value.subRepositories.length} 个子仓库` : '非 Git 项目');
</script>
<template><button type="button" class="ui-workspace-nav" :class="{active}" :aria-current="active?'page':undefined" :title="`${workspace.path}\n${focus.title}\n${state}`" @click="$emit('click')">
<Folder :size="17"/>
<span class="min-w-0">
<strong>{{workspace.name}}</strong>
<small>{{meta}}</small>
<small class="ui-focus-label">{{focus.title}}</small>
<small>{{focus.plan||focus.goal ? statusPresentation(focus.plan?.status??focus.goal?.status).label+' · ' : ''}}{{state}}</small>
</span>
</button></template>
