<script setup lang="ts">
import { ref } from "vue";
import PageShell from "../components/ui/PageShell.vue";
import PageHeader from "../components/ui/PageHeader.vue";
import GlassCard from "../components/ui/GlassCard.vue";
import BaseButton from "../components/ui/BaseButton.vue";
import TextField from "../components/ui/TextField.vue";
import SelectField from "../components/ui/SelectField.vue";
import TextAreaField from "../components/ui/TextAreaField.vue";
import StatusPill from "../components/ui/StatusPill.vue";
import PageTabs from "../components/ui/PageTabs.vue";
import SegmentedControl from "../components/ui/SegmentedControl.vue";
import ToggleSwitch from "../components/ui/ToggleSwitch.vue";
import EmptyState from "../components/ui/EmptyState.vue";
import SaveBar from "../components/ui/SaveBar.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import PalettePicker from "../components/settings/PalettePicker.vue";
const text = ref('可编辑的草稿');
const option = ref('one');
const tab = ref('one');
const mode = ref('direct');
const on = ref(true);
const modal = ref(false);
</script>
<template><section class="page-scroll">
<PageShell>
<PageHeader title="组件展示" kicker="仅开发环境" description="检查浅色、深色、品牌色以及键盘和交互状态。"/>
<GlassCard>
<PalettePicker/>
</GlassCard>
<GlassCard>
<div class="ui-actions">
<BaseButton @click="modal=true">主要操作</BaseButton>
<BaseButton variant="secondary">次要操作</BaseButton>
<BaseButton variant="ghost">文字操作</BaseButton>
<BaseButton variant="danger">危险操作</BaseButton>
<BaseButton disabled>禁用</BaseButton>
<BaseButton busy>处理中</BaseButton>
</div>
<div class="ui-actions mt-4">
<StatusPill v-for="status in ['running','blocked','awaiting_acceptance','completed','stopped','unknown']" :key="status" :status="status"/>
</div>
</GlassCard>
<GlassCard class="ui-stack">
<TextField v-model="text" label="文本字段" hint="辅助信息"/>
<TextField label="字段错误" error="请输入有效值"/>
<SelectField v-model="option" label="下拉选择" :options="[{value:'one',label:'第一项'},{value:'two',label:'第二项'}]"/>
<TextAreaField v-model="text" label="多行文本"/>
<ToggleSwitch v-model="on" label="开关" description="作用域与生效时间"/>
<SegmentedControl v-model="mode" :items="[{value:'direct',label:'直接执行'},{value:'plan',label:'按计划执行'}]"/>
<SaveBar dirty/>
<PageTabs v-model="tab" :items="[{value:'one',label:'第一个页面'},{value:'two',label:'第二个页面'}]">
<p>当前页签：{{tab}}</p>
</PageTabs>
<EmptyState title="暂无记录" description="空状态提供下一步。">
<BaseButton variant="secondary">添加记录</BaseButton>
</EmptyState>
</GlassCard>
<ModalDialog :open="modal" title="浮层" description="Escape 关闭；Tab 限定在对话框中。" @close="modal=false">
<BaseButton @click="modal=false">完成</BaseButton>
</ModalDialog>
</PageShell>
</section></template>
