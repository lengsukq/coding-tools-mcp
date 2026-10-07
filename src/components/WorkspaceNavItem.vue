<script setup lang="ts">
import { computed } from "vue";
import { Folder } from "@lucide/vue";
import type { WorkspaceProfile } from "$lib/types";
import { useWorkspaceStatus } from "$lib/workspace-status-cache";
import { taskFocus, statusPresentation } from "$lib/ui-model";
const props = defineProps<{
    workspace: WorkspaceProfile;
    active?: boolean;
}>();
defineEmits<{
    click: [
    ];
}>();
const { planning, git, query, gitQuery } = useWorkspaceStatus(() => props.workspace.id);
const focus = computed(() => taskFocus(planning.value));
const state = computed(() => query.value.error ? '数据更新失败' : query.value.stale ? '数据已过期' : planning.value ? statusPresentation(planning.value.execution.state).label : '加载中');
const meta = computed(() => gitQuery.value.error ? 'Git 更新失败' : !git.value ? 'Git 加载中' : gitQuery.value.stale ? 'Git 数据过期' : git.value?.available ? `${git.value.branch || '分离 HEAD'} · ${git.value.changedFiles} 个变更` : git.value?.subRepositories.length ? `${git.value.subRepositories.length} 个子仓库` : '非 Git 项目');
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
