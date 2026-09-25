import { getCurrentWindow } from "@tauri-apps/api/window";
import { getWebviewMemorySample, recreateUiWebview } from "$lib/api/ui-memory";

/** How long the window must stay minimized/hidden before a silent UI recreate. */
const HIDDEN_RELOAD_MS = 50 * 60 * 1000;
/** Auto-recreate when WebView working set exceeds this (MB). */
const MEMORY_WARN_MB = 2048;
/** Min gap between any UI recreates. */
const RELOAD_COOLDOWN_MS = 60 * 60 * 1000;
/** How often to sample memory while the window may be visible. */
const SAMPLE_INTERVAL_MS = 5 * 60 * 1000;
/** Tick for hidden/minimized duration tracking. */
const HIDDEN_TICK_MS = 30 * 1000;
/** Give a hidden window one last chance to become visible before rebuilding it. */
const HIDDEN_RECHECK_DELAY_MS = 500;

const LAST_RELOAD_KEY = "ctm.uiMemory.lastReloadAt";

let started = false;
let hiddenSince: number | null = null;
let sampleTimer: ReturnType<typeof setInterval> | null = null;
let hiddenTimer: ReturnType<typeof setInterval> | null = null;
let releasing = false;

/**
 * Recreate the WebView window (replaces Edge WebView2 processes).
 * Rust AppState keeps MCP / FRP running.
 */
export async function reloadUiOnly(reason: string): Promise<void> {
  if (releasing) return;
  releasing = true;
  try {
    localStorage.setItem(LAST_RELOAD_KEY, String(Date.now()));
  } catch {
    // ignore quota / private mode
  }
  console.info(`[ui-memory-guard] recreating UI (${reason}); services untouched`);
  try {
    await recreateUiWebview();
  } catch (error) {
    // Destroy often aborts the invoke; if the window is already gone, nothing to do.
    // Fall back to document reload only when the window is still alive.
    console.warn("[ui-memory-guard] recreate_ui_webview failed, falling back", error);
    try {
      window.location.reload();
    } catch {
      // ignore
    }
  } finally {
    // If recreate succeeded the JS context is gone; if not, allow retry later.
    releasing = false;
  }
}

function lastReloadAt(): number {
  try {
    const raw = localStorage.getItem(LAST_RELOAD_KEY);
    if (!raw) return 0;
    const n = Number(raw);
    return Number.isFinite(n) ? n : 0;
  } catch {
    return 0;
  }
}

function cooldownOk(): boolean {
  return Date.now() - lastReloadAt() >= RELOAD_COOLDOWN_MS;
}

async function isWindowObscured(): Promise<boolean> {
  if (typeof document !== "undefined" && document.visibilityState === "hidden") {
    return true;
  }
  try {
    return await getCurrentWindow().isMinimized();
  } catch {
    return false;
  }
}

function markVisible(): void {
  hiddenSince = null;
}

function markHidden(): void {
  if (hiddenSince === null) {
    hiddenSince = Date.now();
  }
}

async function maybeSilentReload(): Promise<void> {
  if (!(await isWindowObscured())) {
    markVisible();
    return;
  }
  markHidden();
  if (hiddenSince === null || !cooldownOk() || releasing) return;
  if (Date.now() - hiddenSince < HIDDEN_RELOAD_MS) return;

  // The user may be restoring the app at the exact moment the hidden timer
  // fires. Re-check after a short delay so a reveal never races a WebView
  // recreate and produces a visible "restart" flash.
  await new Promise((resolve) => window.setTimeout(resolve, HIDDEN_RECHECK_DELAY_MS));
  if (!(await isWindowObscured())) {
    markVisible();
    return;
  }

  await reloadUiOnly("hidden-or-minimized");
}

async function maybeAutoRecreateHighMemory(): Promise<void> {
  if (releasing) return;
  if (await isWindowObscured()) return;
  if (!cooldownOk()) return;

  let sample;
  try {
    sample = await getWebviewMemorySample();
  } catch {
    return;
  }
  if (!sample.supported) return;
  if (sample.webviewMb < MEMORY_WARN_MB) return;

  await reloadUiOnly(
    `auto-threshold-${Math.round(sample.webviewMb)}mb`,
  );
}

/**
 * Start background UI memory guard. Safe to call once from root layout.
 * Never stops backend services.
 */
export function startUiMemoryGuard(): () => void {
  if (started || typeof window === "undefined") {
    return () => {};
  }
  started = true;

  const onVisibility = () => {
    void (async () => {
      if (await isWindowObscured()) markHidden();
      else markVisible();
    })();
  };

  document.addEventListener("visibilitychange", onVisibility);
  void onVisibility();

  sampleTimer = setInterval(() => {
    void maybeAutoRecreateHighMemory();
  }, SAMPLE_INTERVAL_MS);

  hiddenTimer = setInterval(() => {
    void maybeSilentReload();
  }, HIDDEN_TICK_MS);

  const firstSample = setTimeout(() => {
    void maybeAutoRecreateHighMemory();
  }, 60_000);

  return () => {
    started = false;
    document.removeEventListener("visibilitychange", onVisibility);
    if (sampleTimer) clearInterval(sampleTimer);
    if (hiddenTimer) clearInterval(hiddenTimer);
    clearTimeout(firstSample);
    sampleTimer = null;
    hiddenTimer = null;
  };
}
