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
import SegmentedControl from "../components/ui/SegmentedControl.vue";
import ToggleSwitch from "../components/ui/ToggleSwitch.vue";
import EmptyState from "../components/ui/EmptyState.vue";
import SaveBar from "../components/ui/SaveBar.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import PalettePicker from "../components/settings/PalettePicker.vue";

const text = ref("可编辑的草稿");
const option = ref("one");
const mode = ref("direct");
const on = ref(true);
const modal = ref(false);
</script>

<template>
  <section class="page-scroll">
    <PageShell>
      <PageHeader
        kicker="Design System 2.0 · 仅开发环境"
        title="克制雾面毛玻璃组件画廊"
        description="检验浅色/深色主题、六套品牌色、三层光学层次（悬浮玻璃岛 / Bento 雾面主卡 / 零模糊内嵌层）与键盘交互状态。"
      />

      <div class="ui-bento-grid">
        <GlassCard variant="focal">
          <div class="ui-section-heading">
            <div>
              <h2>品牌强调色调色板</h2>
              <p>切换全局强调色，雾面背景与卡片边缘高光同步响应。</p>
            </div>
          </div>
          <PalettePicker />
        </GlassCard>

        <GlassCard>
          <div class="ui-section-heading">
            <div>
              <h2>按钮与状态胶囊</h2>
              <p>全胶囊/大圆角操作控件与独立语义色徽章。</p>
            </div>
          </div>
          <div class="ui-actions">
            <BaseButton @click="modal = true">主要操作</BaseButton>
            <BaseButton variant="secondary">次要操作</BaseButton>
            <BaseButton variant="ghost">文字操作</BaseButton>
            <BaseButton variant="danger">危险操作</BaseButton>
            <BaseButton disabled>禁用</BaseButton>
            <BaseButton busy>处理中</BaseButton>
          </div>
          <div class="ui-actions mt-4">
            <StatusPill
              v-for="status in ['running', 'blocked', 'awaiting_acceptance', 'completed', 'stopped', 'unknown']"
              :key="status"
              :status="status"
            />
          </div>
        </GlassCard>
      </div>

      <GlassCard class="ui-stack">
        <div class="ui-section-heading">
          <div>
            <h2>表单、胶囊导航与保存反馈</h2>
            <p>内嵌控件采用同心圆角（14px / 18px / 999px），支持完整键盘无障碍导航。</p>
          </div>
        </div>
        <div class="ui-two-col">
          <TextField v-model="text" label="文本字段" hint="支持清晰焦点环与辅助说明" />
          <TextField label="字段校验错误" error="请输入有效的配置值" />
        </div>
        <div class="ui-two-col">
          <SelectField
            v-model="option"
            label="下拉选择"
            :options="[
              { value: 'one', label: '第一项 · 默认推荐' },
              { value: 'two', label: '第二项 · 自定义策略' },
            ]"
          />
          <div>
            <span class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">分段控制器</span>
            <SegmentedControl
              v-model="mode"
              :items="[
                { value: 'direct', label: '直接执行' },
                { value: 'plan', label: '按计划执行' },
                { value: 'goal', label: '目标约束' },
              ]"
            />
          </div>
        </div>
        <TextAreaField v-model="text" label="多行文本" />
        <div class="ui-inset p-4">
          <ToggleSwitch v-model="on" label="雾面内嵌开关项" description="保存后立即热更新应用，无需重启本地服务。" />
        </div>
        <SaveBar dirty />
        <EmptyState title="暂无记录" description="柔和虚线雾面空状态，提供明确的下一步引导。">
          <BaseButton variant="secondary">添加记录</BaseButton>
        </EmptyState>
      </GlassCard>

      <ModalDialog :open="modal" title="雾面大圆角浮层" description="支持 Escape 关闭与焦点环锁定（Focus Trap）。" @close="modal = false">
        <div class="flex justify-end">
          <BaseButton @click="modal = false">完成预览</BaseButton>
        </div>
      </ModalDialog>
    </PageShell>
  </section>
</template>
