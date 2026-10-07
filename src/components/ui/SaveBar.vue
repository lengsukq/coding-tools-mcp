<script setup lang="ts">
import BaseButton from "./BaseButton.vue";
withDefaults(defineProps<{
    dirty: boolean;
    busy?: boolean;
    error?: string;
    label?: string;
}>(), { label: "保存更改" });
defineEmits<{
    save: [
    ];
}>();
</script>
<template><div class="ui-save-bar" role="status">
<span :class="{'ui-danger':!!error}">{{error || (busy ? '正在保存…' : dirty ? '有未保存的更改' : '已保存')}}</span>
<BaseButton :busy="busy" :disabled="!dirty" @click="$emit('save')">{{label}}</BaseButton>
</div></template>
