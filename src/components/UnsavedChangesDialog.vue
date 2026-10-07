<script setup lang="ts">
import { ref } from "vue";
import ModalDialog from "./ui/ModalDialog.vue";
import BaseButton from "./ui/BaseButton.vue";
import { unsavedDialogOpen, unsavedLabels, unsavedError, resolveUnsavedChanges } from "$lib/unsaved-changes";
const busy = ref(false);
async function choose(action: "save" | "discard" | "cancel") { if (busy.value)
    return; busy.value = true; try {
    await resolveUnsavedChanges(action);
}
finally {
    busy.value = false;
} }
</script>
<template><ModalDialog :open="unsavedDialogOpen" title="有未保存的更改" description="离开前可以保存，也可以放弃本次修改。" :dismissible="!busy" @close="choose('cancel')">
<p class="ui-secondary">{{unsavedLabels.join('、')}}</p>
<p v-if="unsavedError" class="ui-danger" role="alert">{{unsavedError}}</p>
<div class="ui-actions justify-end mt-6">
<BaseButton variant="ghost" :disabled="busy" @click="choose('cancel')">取消</BaseButton>
<BaseButton variant="secondary" :disabled="busy" @click="choose('discard')">放弃修改</BaseButton>
<BaseButton :busy="busy" @click="choose('save')">保存并离开</BaseButton>
</div>
</ModalDialog></template>
