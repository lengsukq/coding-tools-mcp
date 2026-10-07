<script setup lang="ts">
import ModalDialog from "../ui/ModalDialog.vue";
import TextField from "../ui/TextField.vue";
defineProps<{
    open: boolean;
    entries: Array<{
        label: string;
        hint: string;
    }>;
}>();
const query = defineModel<string>("query", { default: "" });
const emit = defineEmits<{
    close: [
    ];
    select: [
        index: number
    ];
}>();
</script>
<template><ModalDialog :open="open" title="快捷操作" @close="emit('close')">
<TextField v-model="query" label="搜索工作区或操作" placeholder="输入名称…" @keydown.enter="entries[0] && emit('select',0)"/>
<div class="ui-command-results">
<p v-if="!entries.length" class="ui-muted p-4">没有匹配的操作。</p>
<button v-for="(entry,index) in entries" :key="entry.label" type="button" class="ui-list-action" @click="emit('select',index)">
<strong>{{entry.label}}</strong>
<small>{{entry.hint}}</small>
</button>
</div>
</ModalDialog></template>
