import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import test from "node:test";

const source = await readFile(new URL("../src-tauri/src/mcp/listener.rs", import.meta.url), "utf8");
const htmlTemplate = source.match(/r#"<!doctype html>([\s\S]*?)"#/)?.[1];

const example = {
  id: "split-fixture",
  summary: "Split diff alignment fixture",
  stats: { files: 1, additions: 1, deletions: 1 },
  files: [{
    path: "src/a/very/long/nested/path/file.ts",
    status: "update",
    additions: 1,
    deletions: 1,
    patch: [
      "--- a/src/a/very/long/nested/path/file.ts",
      "+++ b/src/a/very/long/nested/path/file.ts",
      "@@ -10,3 +10,3 @@",
      " context();",
      "-oldVersion();",
      "+newVersion();",
      " const longLine = '" + "x".repeat(260) + "';",
    ].join("\n"),
  }],
};

function element() {
  let html = "";
  return {
    children: [],
    dataset: {},
    classList: { toggle() {}, add() {} },
    style: {},
    get innerHTML() { return html; },
    set innerHTML(value) {
      html = value;
      if (value === "") this.children = [];
    },
    appendChild(child) { this.children.push(child); },
    scrollIntoView() {},
  };
}

function page(mode = "split") {
  assert.ok(htmlTemplate, "review HTML template must exist");
  const rendered = htmlTemplate
    .replaceAll("{{", "{").replaceAll("}}", "}")
    .replace("{data}", JSON.stringify(example));
  const script = rendered.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(script, "review page must have executable JS");

  const items = Object.fromEntries(
    ["summary", "files", "diff", "unified", "split", "filter", "status", "ws",
      "collapse", "prev", "next"].map(key => [key, element()]),
  );
  Object.assign(items.filter, { value: "" });
  Object.assign(items.status, { value: "" });
  Object.assign(items.ws, { checked: false });
  const context = {
    ...items,
    document: {
      querySelector: () => ({ offsetHeight: 130 }),
      documentElement: { style: { setProperty() {} } },
      querySelectorAll: () => items.diff.children,
      createElement: () => element(),
      activeElement: null,
    },
    ResizeObserver: class { observe() {} },
    getComputedStyle: () => ({ top: "0px" }),
    addEventListener() {},
    innerWidth: 1440,
    localStorage: { reviewMode: mode, getItem: () => "[]", setItem() {} },
  };
  runInNewContext(script, context, { timeout: 2000 });
  return { context, rendered, items };
}

test("split renders two isolated scroll panes with matching fixed-height rows", () => {
  const { items, rendered } = page();
  const markup = items.diff.children[0].innerHTML;
  assert.equal((markup.match(/class="split-pane"/g) ?? []).length, 2);
  assert.equal((markup.match(/class="split-code"/g) ?? []).length, 2);
  const panes = [...markup.matchAll(/<pre class="split-code"[^>]*>([\s\S]*?)<\/pre>/g)];
  assert.equal(panes.length, 2);
  const oldRows = panes[0][1].match(/class="line\b/g) ?? [];
  const newRows = panes[1][1].match(/class="line\b/g) ?? [];
  assert.equal(oldRows.length, newRows.length);
  assert.equal(oldRows.length, 4);
  assert.match(panes[0][1], /oldVersion\(\)/);
  assert.doesNotMatch(panes[0][1], /newVersion\(\)/);
  assert.match(panes[1][1], /newVersion\(\)/);
  assert.doesNotMatch(panes[1][1], /oldVersion\(\)/);
  assert.doesNotMatch(panes[0][1], /--- a\/src/);
  assert.match(rendered, /\.split-code\{[^}]*overflow-x:auto/);
  assert.match(rendered, /\.split-code \.line\{[^}]*height:20px/);
  assert.match(rendered, /\.fh \.filename\{[^}]*text-overflow:ellipsis/);
});

test("mode switching preserves existing unified diff rendering", () => {
  const { items } = page();
  items.unified.onclick();
  let markup = items.diff.children[0].innerHTML;
  assert.doesNotMatch(markup, /class="split-pane"/);
  assert.match(markup, /<pre>/);
  items.split.onclick();
  markup = items.diff.children[0].innerHTML;
  assert.equal((markup.match(/class="split-pane"/g) ?? []).length, 2);
});
