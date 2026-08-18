import { invoke } from "@tauri-apps/api/core";

export interface HistorySnippet {
  turn_id: string;
  timestamp: string;
  text: string;
}

export interface HistorySessionSummary {
  number: number;
  path: string;
  title: string;
  session_key?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  bytes: number;
  entry_count: number;
  latest_focus: string;
  key_files: string[];
  snippets: HistorySnippet[];
}

export interface HistorySessionCatalog {
  context_revision: string;
  history_count: number;
  sessions: HistorySessionSummary[];
}

export async function listHistorySessions(workspaceId: string): Promise<HistorySessionCatalog> {
  return invoke<HistorySessionCatalog>("list_history_sessions", { id: workspaceId });
}
