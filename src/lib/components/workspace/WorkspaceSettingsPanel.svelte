<script lang="ts">
  import { Trash2 } from "@lucide/svelte";
  import ChatGptSessionPrompt from "$lib/components/ChatGptSessionPrompt.svelte";
  import WorkspaceMetaForm from "$lib/components/WorkspaceMetaForm.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import type { WorkspaceProfile } from "$lib/types";

  interface Props {
    profile: WorkspaceProfile;
    onSaveName: (name: string) => void | Promise<void>;
    onUpdatePath: (path: string) => void | Promise<void>;
    onDelete: () => void;
  }

  let { profile, onSaveName, onUpdatePath, onDelete }: Props = $props();
</script>

<div class="grid gap-6 max-w-4xl">
  <div>
    <h3 class="text-sm font-semibold text-[var(--text-main)]">工作区基础设置</h3>
    <p class="text-xs text-[var(--color-text-muted)] mt-0.5">维护工作区展示名称、物理存储目录与 ChatGPT 初始 Prompt</p>
  </div>

  <Card class="p-5">
    <h4 class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider mb-4">基本属性</h4>
    <WorkspaceMetaForm
      name={profile.name}
      path={profile.path}
      onSave={onSaveName}
      onUpdatePath={onUpdatePath}
    />
  </Card>

  <Card class="p-5">
    <h4 class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider mb-2">ChatGPT 初始会话指令</h4>
    <p class="text-xs text-[var(--color-text-muted)] mb-4 leading-relaxed">
      为外部 ChatGPT 会话生成一键粘贴的系统初始化 Prompt，指引大模型连接当前工作区并调用工具。
    </p>
    <ChatGptSessionPrompt />
  </Card>

  <div class="rounded-2xl border border-[var(--danger)]/30 bg-[var(--danger-soft)] p-5 shadow-sm">
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h4 class="text-sm font-bold text-[var(--danger)]">删除工作区</h4>
        <p class="text-xs text-[var(--text-secondary)] mt-1 leading-relaxed">
          仅从 Coding Tools 中移除该工作区配置、历史日志与路由，绝不会删除本地磁盘上的任何源代码项目文件。
        </p>
      </div>
      <Button variant="danger" size="md" onclick={onDelete}>
        <Trash2 size={14} />
        <span>删除工作区</span>
      </Button>
    </div>
  </div>
</div>
