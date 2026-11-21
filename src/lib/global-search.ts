import type { WorkspaceProfile } from "./types";

export type SearchCategory = "设置页面" | "工作区" | "页面与操作";

interface PrecomputedSearchIndex {
  titleLower: string;
  subLower: string;
  kwJoined: string;
  kwExact: Set<string>;
  haystack: string;
}

export interface GlobalSearchEntry {
  id: string;
  category: SearchCategory;
  title: string;
  subtitle: string;
  keywords: string[];
  badge?: string;
  to?: { path: string; query?: Record<string, string> };
  action?: "add-workspace" | "toggle-theme" | "compact-ui";
  /** Precomputed lowercase search fields to avoid per-keystroke allocations */
  _index?: PrecomputedSearchIndex;
}

function computeSearchIndex(entry: Pick<GlobalSearchEntry, "title" | "subtitle" | "keywords">): PrecomputedSearchIndex {
  const titleLower = entry.title.toLowerCase();
  const subLower = entry.subtitle.toLowerCase();
  const lowerKeywords = entry.keywords.map((keyword) => keyword.toLowerCase());
  const kwJoined = lowerKeywords.join(" ");
  return {
    titleLower,
    subLower,
    kwJoined,
    kwExact: new Set(lowerKeywords),
    haystack: `${titleLower} ${subLower} ${kwJoined}`,
  };
}

export interface SettingsNavItem {
  id: string;
  section: "general" | "connection" | "execution" | "computer" | "tools" | "about";
  panel?: string;
  label: string;
  subtitle: string;
  keywords: string[];
}

export interface SettingsNavGroup {
  group: string;
  items: SettingsNavItem[];
}

export const SETTINGS_SIDEBAR_GROUPS: SettingsNavGroup[] = [
  {
    group: "连接与多机互联",
    items: [
      {
        id: "gateway",
        section: "connection",
        panel: "gateway",
        label: "网关与公网隧道",
        subtitle: "本地 MCP 启停、端口、Cloudflare / FRP 隧道与健康检查",
        keywords: ["mcp", "gateway", "port", "端口", "公网", "隧道", "cloudflare", "健康检查", "health", "endpoint", "端点"],
      },
      {
        id: "gemini",
        section: "connection",
        panel: "gemini",
        label: "Gemini 客户端接入",
        subtitle: "Gemini 云端 HTTPS OAuth 与本机 Local MCP stdio 桥接向导",
        keywords: ["gemini", "google", "stdio", "local mcp", "bridge", "桥接", "connected apps", "dcr"],
      },
      {
        id: "auth",
        section: "connection",
        panel: "auth",
        label: "服务名与访问控制",
        subtitle: "自定义 MCP 服务名、认证模式、本机免授权与局域网直连开关",
        keywords: ["名称", "名字", "server name", "自定义", "oauth", "bearer", "noauth", "免授权", "允许局域网访问", "lan", "127.0.0.1"],
      },
      {
        id: "lan",
        section: "connection",
        panel: "lan",
        label: "本机共享与配对",
        subtitle: "生成 6 位配对码、本机设备名、工作区共享范围与 EasyTier 网段",
        keywords: ["局域网", "配对码", "pin", "easytier", "tailscale", "zerotier", "wireguard", "cidr", "共享工作区", "本机设备"],
      },
      {
        id: "lan-peers",
        section: "connection",
        panel: "lan-peers",
        label: "设备发现与管理",
        subtitle: "扫描附近电脑、手动探测 IP 节点与管理已配对跨平台设备",
        keywords: ["局域网", "扫描", "发现", "已配对", "远端设备", "mac mini", "windows", "peer", "多机", "联邦", "探测"],
      },
      {
        id: "keys",
        section: "connection",
        panel: "keys",
        label: "共享认证密钥",
        subtitle: "管理 OAuth Client ID / Secret 与 Bearer Token 凭据",
        keywords: ["密钥", "key", "secret", "token", "bearer", "client id", "凭据", "密码"],
      },
      {
        id: "frp",
        section: "connection",
        panel: "frp",
        label: "FRP 服务器档案",
        subtitle: "新增与编辑自建 FRP 服务端地址、端口与子域名配置",
        keywords: ["frp", "frps", "frpc", "内网穿透", "服务器", "子域名", "subdomain"],
      },
    ],
  },
  {
    group: "执行与桌面操控",
    items: [
      {
        id: "permissions",
        section: "execution",
        panel: "permissions",
        label: "权限与命令白名单",
        subtitle: "新工作区默认权限模式、允许执行的系统命令与全局 PATH",
        keywords: ["权限", "受信任", "安全受限", "白名单", "命令", "command", "path", "环境变量", "执行路径"],
      },
      {
        id: "context",
        section: "execution",
        panel: "context",
        label: "AI 指令与技能来源",
        subtitle: "全局 AI 规则提示词及 Claude / Cursor / Codex 技能源自动扫描",
        keywords: ["ai 指令", "instructions", "skills", "技能", "claude", "cursor", "codex", "opencode", "提示词", "扫描"],
      },
      {
        id: "computer-control",
        section: "computer",
        panel: "control",
        label: "桌面屏幕授权",
        subtitle: "屏幕录制与辅助功能权限预检、本机主动授权与高风险动作审批",
        keywords: ["computer use", "屏幕", "录屏", "截图", "辅助功能", "accessibility", "桌面控制", "点击", "授权"],
      },
      {
        id: "computer-audit",
        section: "computer",
        panel: "audit",
        label: "桌面操控记录",
        subtitle: "独立查看近期屏幕查看与桌面操控调用审计明细",
        keywords: ["computer use", "computer audit", "屏幕审计", "操控记录", "截图日志"],
      },
    ],
  },
  {
    group: "网络与工具组件",
    items: [
      {
        id: "proxy",
        section: "tools",
        panel: "proxy",
        label: "网络代理",
        subtitle: "配置系统代理或自定义 HTTP / HTTPS / SOCKS5 代理地址",
        keywords: ["代理", "proxy", "socks5", "http", "7890", "网络"],
      },
      {
        id: "software",
        section: "tools",
        panel: "software",
        label: "内置隧道组件",
        subtitle: "按需下载、检查或更新 cloudflared 与 frpc 客户端二进制",
        keywords: ["cloudflared", "frpc", "二进制", "组件", "下载", "安装", "software"],
      },
    ],
  },
  {
    group: "偏好与系统",
    items: [
      {
        id: "general",
        section: "general",
        label: "外观与启动",
        subtitle: "界面品牌强调色主题面板与应用启动自动恢复 MCP 服务",
        keywords: ["外观", "主题", "颜色", "强调色", "palette", "启动", "开机", "恢复"],
      },
      {
        id: "about",
        section: "about",
        label: "关于与系统维护",
        subtitle: "检查新版本更新、采样释放 WebView 界面内存与轻量驻留",
        keywords: ["关于", "版本", "更新", "update", "内存", "webview", "释放内存", "轻量驻留", "菜单栏"],
      },
    ],
  },
];

export interface WorkspaceSubNavItem {
  id: string;
  tab: "overview" | "planning" | "changes" | "history" | "diagnostics" | "settings";
  panel?: "project" | "policy" | "context";
  label: string;
  subtitle: string;
  keywords: string[];
}

export const WORKSPACE_SIDEBAR_GROUPS: Array<{ group: string; items: WorkspaceSubNavItem[] }> = [
  {
    group: "开发与审阅",
    items: [
      { id: "overview", tab: "overview", label: "工作区概览", subtitle: "当前焦点目标、Git 状态与快速入口", keywords: ["overview", "概览", "状态", "分支"] },
      { id: "planning", tab: "planning", label: "任务与验收", subtitle: "目标（Goal）、计划（Plan）步骤进度与人工验收", keywords: ["planning", "任务", "计划", "目标", "验收", "goal", "plan"] },
      { id: "changes", tab: "changes", label: "代码变更审阅", subtitle: "工作树未提交文件与 AI 修改前后 Diff 快照对比", keywords: ["changes", "变更", "diff", "git", "代码审查", "review"] },
      { id: "history", tab: "history", label: "历史会话上下文", subtitle: "历史检查点归档与跨会话上下文注入选择", keywords: ["history", "历史", "会话", "checkpoint", "上下文"] },
      { id: "diagnostics", tab: "diagnostics", label: "运行诊断日志", subtitle: "工作区运行时事件流与终端异常排查", keywords: ["diagnostics", "诊断", "日志", "log", "报错", "错误"] },
    ],
  },
  {
    group: "工作区配置",
    items: [
      { id: "settings-project", tab: "settings", panel: "project", label: "项目信息与保护", subtitle: "显示名称、目录路径、高风险文件保护与移除工作区", keywords: ["项目", "路径", "名称", "高风险", "保护", "删除", "提示词", "chatgpt"] },
      { id: "settings-policy", tab: "settings", panel: "policy", label: "执行权限与命令", subtitle: "工具档位、权限覆盖、命令白名单、PATH 与本地脚本", keywords: ["权限", "白名单", "命令", "path", "脚本", "档位", "compact"] },
      { id: "settings-context", tab: "settings", panel: "context", label: "AI 指令与历史记录", subtitle: "工作区专属规则、技能来源扫描与历史记录开关", keywords: ["ai 指令", "技能", "skills", "扫描", "记录历史"] },
    ],
  },
];

export function resolveActiveSettingsItemId(section: string, panel?: string): string {
  if (section === "connection") {
    if (panel === "health" || !panel || panel === "gateway") return "gateway";
    if (["gemini", "auth", "lan", "lan-peers", "keys", "frp"].includes(panel)) return panel;
    if (panel === "auth-scope") return "auth";
    if (panel === "gemini-connector") return "gemini";
    return "gateway";
  }
  if (section === "execution") {
    return panel === "context" ? "context" : "permissions";
  }
  if (section === "computer") {
    return panel === "audit" ? "computer-audit" : "computer-control";
  }
  if (section === "tools") {
    return panel === "software" ? "software" : "proxy";
  }
  if (section === "about") return "about";
  return "general";
}

export function buildGlobalSearchEntries(
  workspaces: WorkspaceProfile[],
  options: { canCompact?: boolean } = {},
): GlobalSearchEntry[] {
  const entries: GlobalSearchEntry[] = [
    {
      id: "page-dashboard",
      category: "页面与操作",
      title: "工作台总览",
      subtitle: "查看当前焦点任务、待验收事项与全部工作区卡片",
      keywords: ["首页", "总览", "dashboard", "home", "工作台"],
      badge: "页面",
      to: { path: "/" },
    },
    {
      id: "page-audit",
      category: "页面与操作",
      title: "工具调用审计",
      subtitle: "检索全局工具调用记录、耗时、跨机路由与清理策略",
      keywords: ["审计", "日志", "audit", "log", "调用记录", "历史调用"],
      badge: "页面",
      to: { path: "/audit" },
    },
    {
      id: "action-add-workspace",
      category: "页面与操作",
      title: "添加新工作区…",
      subtitle: "选择本地代码仓库目录并注册到 Coding Tools MCP",
      keywords: ["添加", "新增", "新建", "导入", "workspace", "项目", "仓库"],
      badge: "操作",
      action: "add-workspace",
    },
    {
      id: "action-toggle-theme",
      category: "页面与操作",
      title: "切换浅色 / 深色外观模式",
      subtitle: "立即切换全局雾面毛玻璃深浅色主题",
      keywords: ["深色", "浅色", "暗黑", "夜间", "dark", "light", "theme", "主题切换"],
      badge: "操作",
      action: "toggle-theme",
    },
  ];

  if (options.canCompact) {
    entries.push({
      id: "action-compact-ui",
      category: "页面与操作",
      title: "进入轻量驻留（释放主窗口界面内存）",
      subtitle: "关闭界面窗口并在 macOS 菜单栏保持后台 MCP 服务运行",
      keywords: ["轻量驻留", "后台", "菜单栏", "托盘", "compact", "省内存"],
      badge: "操作",
      action: "compact-ui",
    });
  }

  for (const group of SETTINGS_SIDEBAR_GROUPS) {
    for (const item of group.items) {
      entries.push({
        id: `settings-${item.id}`,
        category: "设置页面",
        title: item.label,
        subtitle: item.subtitle,
        keywords: [group.group, ...item.keywords],
        badge: group.group,
        to: {
          path: `/settings/${item.section}`,
          ...(item.panel ? { query: { panel: item.panel } } : {}),
        },
      });
    }
  }

  for (const ws of workspaces) {
    entries.push({
      id: `ws-${ws.id}`,
      category: "工作区",
      title: ws.name,
      subtitle: ws.path,
      keywords: [ws.name, ws.path, "workspace", "工作区", "项目"],
      badge: "概览",
      to: { path: `/workspace/${ws.id}/overview` },
    });

    for (const group of WORKSPACE_SIDEBAR_GROUPS) {
      for (const sub of group.items) {
        if (sub.id === "overview") continue;
        entries.push({
          id: `ws-${ws.id}-${sub.id}`,
          category: "工作区",
          title: `${ws.name} · ${sub.label}`,
          subtitle: `${sub.subtitle}（${ws.path}）`,
          keywords: [ws.name, ws.path, sub.label, ...sub.keywords],
          badge: group.group,
          to: {
            path: `/workspace/${ws.id}/${sub.tab}`,
            ...(sub.panel ? { query: { panel: sub.panel } } : {}),
          },
        });
      }
    }
  }

  for (const entry of entries) {
    entry._index = computeSearchIndex(entry);
  }
  return entries;
}

const DEFAULT_FEATURED_SETTINGS_IDS = new Set([
  "settings-gateway",
  "settings-gemini",
  "settings-lan",
  "settings-auth",
  "settings-permissions",
  "settings-computer-control",
]);

export function filterGlobalSearchEntries(
  entries: GlobalSearchEntry[],
  rawQuery: string,
  limit = 24,
): GlobalSearchEntry[] {
  const q = rawQuery.trim().toLowerCase();
  if (!q) {
    // Default zero-query view: top quick pages/actions + primary workspaces + top settings
    const primary = entries.filter(
      (item) =>
        item.category === "页面与操作" ||
        (item.category === "工作区" && item.badge === "概览") ||
        DEFAULT_FEATURED_SETTINGS_IDS.has(item.id),
    );
    return primary.slice(0, limit);
  }

  const tokens = q.split(/\s+/).filter(Boolean);
  const scored: Array<{ entry: GlobalSearchEntry; score: number }> = [];

  for (const entry of entries) {
    const idx = entry._index ?? computeSearchIndex(entry);
    if (!tokens.every((token) => idx.haystack.includes(token))) continue;

    let score = 0;
    for (const token of tokens) {
      if (idx.titleLower === token) score += 120;
      else if (idx.titleLower.startsWith(token)) score += 80;
      else if (idx.titleLower.includes(token)) score += 50;
      if (idx.kwExact.has(token)) score += 60;
      else if (idx.kwJoined.includes(token)) score += 30;
      if (idx.subLower.includes(token)) score += 15;
    }
    // Prefer primary workspace overview over deep sub-tabs when query only names the workspace
    if (entry.category === "工作区" && entry.badge === "概览") score += 12;
    if (entry.category === "设置页面") score += 8;

    scored.push({ entry, score });
  }

  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, limit).map((item) => item.entry);
}
