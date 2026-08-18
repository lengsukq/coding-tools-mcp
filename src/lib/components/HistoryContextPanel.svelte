<script lang="ts">
  import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
  import { readWorkspaceLogs } from "$lib/api/logs";

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
      <p class="mt-1 text-sm text-[var(--color-text-muted)]">
        当前会话默认记录；旧会话只有在这里选择后，才会以索引和精选片段注入 MCP 上下文。
      </p>
    </div>
    <button type="button" class="tx-btn-ghost" disabled={loading} onclick={() => void refresh()}>
      {loading ? "刷新中…" : "刷新历史"}
    </button>
  </div>

  <label class="flex items-start gap-2 rounded-md border border-[var(--color-border)] p-3 text-sm">
    <input type="checkbox" bind:checked={draftRecording} />
    <span>
      <span class="block font-medium">记录当前会话</span>
      <span class="mt-1 block text-xs text-[var(--color-text-muted)]">关闭后仍可读取旧历史，但不会追加新的检查点。</span>
    </span>
  </label>

  {#if error}
    <p class="text-xs text-[var(--danger)]">{error}</p>
  {:else if !loading && sessions.length === 0}
    <div class="rounded-md border border-dashed border-[var(--color-border)] p-4 text-sm text-[var(--color-text-muted)]">
      当前工作区还没有历史会话。保存一次任务后，这里会出现可选择的记录。
    </div>
  {:else}
    <div class="grid min-w-0 gap-2">
      <div class="flex items-center justify-between text-xs text-[var(--color-text-muted)]">
        <span>选择要注入的历史会话（已选 {draftSelected.length} 个）</span>
        {#if draftSelected.length > 0}
          <button type="button" class="underline" onclick={() => (draftSelected = [])}>清除选择</button>
        {/if}
      </div>
      {#each sessions as session}
        <div class="min-w-0 max-w-full overflow-hidden rounded-md border border-[var(--color-border)] p-3">
          <label class="flex min-w-0 items-start gap-2">
            <input
              type="checkbox"
              class="mt-0.5 shrink-0"
              checked={draftSelected.includes(session.number)}
              onchange={(event) => toggleSession(session.number, event.currentTarget.checked)}
            />
            <span class="min-w-0 flex-1 overflow-hidden">
              <span class="flex flex-wrap items-center gap-2 text-sm font-medium">
                <span class="min-w-0 max-w-full break-words">#{session.number} {session.title}</span>
                <span class="shrink-0 text-[11px] font-normal text-[var(--color-text-muted)]">
                  {session.entry_count} 条 · {session.bytes} B
                </span>
              </span>
              <span class="mt-1 block min-w-0 truncate text-[11px] text-[var(--color-text-muted)]">
                {session.updated_at ?? session.created_at ?? "时间未知"}
              </span>
              <span class="mt-1 block min-w-0 truncate text-xs text-[var(--color-text-muted)]">{session.latest_focus}</span>
              {#if session.key_files.length > 0}
                <span class="mt-1 block min-w-0 truncate text-[11px] text-[var(--color-text-muted)]">
                  文件：{session.key_files.join("、")}
                </span>
              {/if}
            </span>
            <button
              type="button"
              class="shrink-0 whitespace-nowrap text-xs text-[var(--color-accent)]"
              onclick={() => (expanded = expanded === session.number ? null : session.number)}
            >
              {expanded === session.number ? "收起" : "预览"}
            </button>
          </label>
          {#if expanded === session.number && session.snippets.length > 0}
            <div class="mt-3 grid gap-2 border-t border-[var(--color-border)] pt-3">
              {#each session.snippets as snippet}
                <p class="min-w-0 break-words text-xs leading-5 text-[var(--color-text-muted)]">{snippet.text}</p>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="rounded-md border border-[var(--color-border)] p-3">
    <p class="text-sm font-medium">上下文审计</p>
    <p class="mt-1 text-xs text-[var(--color-text-muted)]">
      只记录块大小、哈希和重复标记，不记录完整提示词。最近 MCP 请求：
    </p>
    {#if auditLines.length > 0}
      <div class="mt-2 grid gap-1 font-mono text-[11px] text-[var(--color-text-muted)]">
        {#each auditLines as line}
          <p class="truncate">{line}</p>
        {/each}
      </div>
    {:else}
      <p class="mt-2 text-xs text-[var(--color-text-muted)]">暂无审计记录；启动 MCP 后会显示最近请求。</p>
    {/if}
  </div>

  <div class="flex items-center justify-between gap-3 border-t border-[var(--color-border)] pt-3">
    <span class="text-xs text-[var(--color-text-muted)]">修改选择后需要刷新 MCP 上下文才会生效。</span>
    <button type="button" class="tx-btn-primary" disabled={saving} onclick={() => void save()}>
      {saving ? "保存中…" : "应用历史上下文"}
    </button>
  </div>
</div>
