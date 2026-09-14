<script lang="ts">
  import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
  import { readWorkspaceLogs } from "$lib/api/logs";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";

  interface Props {
    workspaceId: string;
    recording: boolean;
    selectedSessions: number[];
    onSave: (recording: boolean, selectedSessions: number[]) => void | Promise<void>;
  }

  let {
    workspaceId,
    recording,
    selectedSessions,
    onSave,
  }: Props = $props();

  let sessions = $state<HistorySessionSummary[]>([]);
  let draftRecording = $state(true);
  let draftSelected = $state<number[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state("");
  let expanded = $state<number | null>(null);
  let auditLines = $state<string[]>([]);

  $effect(() => {
    draftRecording = recording;
    draftSelected = [...selectedSessions];
  });

  $effect(() => {
    if (workspaceId) void refresh();
  });

  async function refresh() {
    if (loading) return;
    loading = true;
    error = "";
    try {
      const catalog = await listHistorySessions(workspaceId);
      sessions = catalog.sessions;
      try {
        const logs = await readWorkspaceLogs(workspaceId, "mcp");
        const auditLog = logs.find((chunk) => chunk.name === "mcp-requests.log")?.content ?? "";
        auditLines = auditLog
          .split("\n")
          .filter((line) => line.includes("[context-audit]"))
          .slice(-6)
          .reverse();
      } catch {
        auditLines = [];
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  function toggleSession(number: number, checked: boolean) {
    draftSelected = checked
      ? [...new Set([...draftSelected, number])]
      : draftSelected.filter((item) => item !== number);
  }

  async function save() {
    if (saving) return;
    saving = true;
    try {
      await onSave(draftRecording, [...draftSelected]);
    } finally {
      saving = false;
    }
  }
</script>

<div class="grid gap-4">
  <div class="flex items-start justify-between gap-3">
    <div>
      <p class="tx-section-label">历史上下文</p>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        当前会话默认记录；旧会话只有在这里选择后，才会以索引和精选片段注入 MCP 上下文。
      </p>
    </div>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      disabled={loading}
      busy={loading}
      onclick={() => void refresh()}
    >
      {loading ? "刷新中…" : "刷新历史"}
    </Button>
  </div>

  <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
    <Toggle
      bind:checked={draftRecording}
      label="记录当前会话"
      description="关闭后仍可读取旧历史，但不会追加新的检查点。"
    />
  </div>

  {#if error}
    <p class="text-xs text-[var(--danger)]">{error}</p>
  {:else if !loading && sessions.length === 0}
    <div class="rounded-xl border border-dashed border-[var(--border)] p-4 text-xs text-[var(--color-text-muted)]">
      当前工作区还没有历史会话。保存一次任务后，这里会出现可选择的记录。
    </div>
  {:else}
    <div class="grid min-w-0 gap-2">
      <div class="flex items-center justify-between text-xs text-[var(--color-text-muted)]">
        <span>选择要注入的历史会话（已选 {draftSelected.length} 个）</span>
        {#if draftSelected.length > 0}
          <button type="button" class="underline hover:text-[var(--text-main)]" onclick={() => (draftSelected = [])}>清除选择</button>
        {/if}
      </div>
      {#each sessions as session}
        <div class="min-w-0 max-w-full overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-3 shadow-sm">
          <label class="flex min-w-0 items-start gap-2.5 cursor-pointer">
            <input
              type="checkbox"
              class="mt-1 shrink-0 rounded border-[var(--border)] text-[var(--primary)] focus:ring-[var(--primary)]/30"
              checked={draftSelected.includes(session.number)}
              onchange={(event) => toggleSession(session.number, event.currentTarget.checked)}
            />
            <span class="min-w-0 flex-1 overflow-hidden">
              <span class="flex flex-wrap items-center gap-2 text-xs font-semibold text-[var(--text-main)]">
                <span class="min-w-0 max-w-full break-words">#{session.number} {session.title}</span>
                <span class="shrink-0 text-[11px] font-normal text-[var(--color-text-muted)]">
                  {session.entry_count} 条 · {session.bytes} B
                </span>
              </span>
              <span class="mt-1 block min-w-0 truncate text-[11px] text-[var(--color-text-muted)]">
                {session.updated_at ?? session.created_at ?? "时间未知"}
              </span>
              <span class="mt-1 block min-w-0 truncate text-xs text-[var(--text-secondary)]">{session.latest_focus}</span>
              {#if session.key_files.length > 0}
                <span class="mt-1 block min-w-0 truncate text-[11px] text-[var(--color-text-muted)]">
                  文件：{session.key_files.join("、")}
                </span>
              {/if}
            </span>
            <button
              type="button"
              class="shrink-0 whitespace-nowrap text-xs text-[var(--color-accent)] hover:underline"
              onclick={(e) => { e.preventDefault(); expanded = expanded === session.number ? null : session.number; }}
            >
              {expanded === session.number ? "收起" : "预览"}
            </button>
          </label>
          {#if expanded === session.number && session.snippets.length > 0}
            <div class="mt-3 grid gap-1.5 border-t border-[var(--border)] pt-2.5 font-mono text-[11px] leading-relaxed text-[var(--color-text-muted)]">
              {#each session.snippets as snippet}
                <p class="min-w-0 break-words rounded bg-[var(--surface-main)] p-2">{snippet.text}</p>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-3.5 shadow-sm">
    <p class="text-xs font-semibold text-[var(--text-main)]">上下文审计</p>
    <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">
      只记录块大小、哈希和重复标记，不记录完整提示词。最近 MCP 请求：
    </p>
    {#if auditLines.length > 0}
      <div class="mt-2.5 grid gap-1 font-mono text-[11px] text-[var(--text-secondary)]">
        {#each auditLines as line}
          <p class="truncate rounded bg-[var(--card-bg)] px-2.5 py-1 border border-[var(--border)]">{line}</p>
        {/each}
      </div>
    {:else}
      <p class="mt-2 text-xs text-[var(--color-text-muted)]">暂无审计记录；启动 MCP 后会显示最近请求。</p>
    {/if}
  </div>

  <div class="flex items-center justify-between gap-3 border-t border-[var(--border)] pt-3">
    <span class="text-xs text-[var(--color-text-muted)]">修改选择后需要刷新 MCP 上下文才会生效。</span>
    <Button
      type="button"
      variant="primary"
      disabled={saving}
      busy={saving}
      onclick={() => void save()}
    >
      {saving ? "保存中…" : "应用历史上下文"}
    </Button>
  </div>
</div>
