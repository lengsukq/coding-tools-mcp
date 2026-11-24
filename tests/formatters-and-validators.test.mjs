import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

async function loadTsModule(relativePath) {
  const src = await readFile(new URL(`../${relativePath}`, import.meta.url), "utf8");
  const jsCode = ts.transpileModule(src, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  return import(`data:text/javascript,${encodeURIComponent(jsCode)}`);
}

test("formatters handle invalid dates, relative thresholds, durations, bytes, and errors", async () => {
  const fmt = await loadTsModule("src/lib/formatters.ts");

  assert.equal(fmt.formatFullDateTime("not-a-date"), "not-a-date");
  assert.equal(fmt.formatShortDateTime("not-a-date"), "not-a-date");
  assert.equal(fmt.formatShortMonthDay("not-a-date"), "not-a-date");
  assert.equal(fmt.formatTimeOnly("not-a-date"), "not-a-date");
  assert.match(fmt.formatTimeOnly(Date.UTC(2026, 0, 2, 3, 4, 5)), /\d{2}:\d{2}:\d{2}/);

  const now = Date.now();
  assert.equal(fmt.formatRelativeTime(now - 5_000), "刚刚");
  assert.equal(fmt.formatRelativeTime(now - 30_000), "30秒前");
  assert.equal(fmt.formatRelativeTime(now - 5 * 60_000), "5分钟前");
  assert.equal(fmt.formatRelativeTime(now - 3 * 3600_000), "3小时前");
  assert.equal(fmt.formatRelativeTime(now - 4 * 86_400_000), "4天前");

  assert.equal(fmt.formatDuration(450), "450 ms");
  assert.equal(fmt.formatDuration(1250), "1.25 s");
  assert.equal(fmt.formatDuration(12_340), "12.3 s");

  assert.equal(fmt.formatBytes(null), "0 B");
  assert.equal(fmt.formatBytes(0), "0 B");
  assert.equal(fmt.formatBytes(512), "512 B");
  assert.equal(fmt.formatBytes(1536), "1.5 KB");
  assert.equal(fmt.formatBytes(2 * 1024 * 1024), "2.00 MB");

  assert.equal(fmt.formatErrorMessage(new Error("boom")), "boom");
  assert.equal(fmt.formatErrorMessage("plain error"), "plain error");
  assert.equal(fmt.formatErrorMessage(404), "404");
});

test("validators enforce strict TCP port bounds and supported proxy schemes", async () => {
  const validators = await loadTsModule("src/lib/validators.ts");

  assert.equal(validators.isValidPort(1), true);
  assert.equal(validators.isValidPort(28765), true);
  assert.equal(validators.isValidPort(65535), true);
  assert.equal(validators.isValidPort(0), false);
  assert.equal(validators.isValidPort(65536), false);
  assert.equal(validators.isValidPort(80.5), false);
  assert.equal(validators.isValidPort("abc"), false);

  assert.equal(validators.isValidProxyUrl("http://127.0.0.1:7890"), true);
  assert.equal(validators.isValidProxyUrl("https://proxy.example.com:8443"), true);
  assert.equal(validators.isValidProxyUrl("socks5://127.0.0.1:1080"), true);
  assert.equal(validators.isValidProxyUrl("socks5h://127.0.0.1:1080"), true);
  assert.equal(validators.isValidProxyUrl("ftp://127.0.0.1:21"), false);
  assert.equal(validators.isValidProxyUrl(""), false);
});
