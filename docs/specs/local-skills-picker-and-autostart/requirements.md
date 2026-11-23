# 需求文档：local-skills-picker-and-autostart

## 功能概述

解决当前版本在本机 Skills 扫描中因未跟随符号链接、未解析 YAML 多行块标量 frontmatter 以及未展示已扫描列表导致的“扫不到/显示不全”问题；重构全局与工作区上下文配置界面，默认隐藏客户端 Harness 选择列表并直接展示可逐条勾选注入的 Skills 与 Agents（Instructions）列表；同时新增跨平台开机自动启动及开机轻量启动（仅驻留菜单栏/托盘）能力。

## 范围边界

- In Scope:
  - 修复 `agent_context` 符号链接遍历、YAML 多行 frontmatter 解析、长描述安全截断与 `~/.agents/skills` / `~/.gemini/` 等本机目录覆盖。
  - 扩展 `scan_global_agent_context` 与 `scan_agent_context`，返回完整去重的 Skills 与 Instructions 列表及稳定 ID。
  - 新增全局与工作区级按条禁用列表持久化（`disabled_skill_ids`、`disabled_instruction_ids`），在 `ToolContext`、`list_skills`、`get_skill`、`workspace_select` 与 `initialize` 生效。
  - 重构全局 `GeneralContextCard.vue` 与工作区 `RuntimePolicyPanel.vue`，默认隐藏 Harness 勾选列表（收入折叠高级区域），直接展示可逐条勾选注入/不注入的 Skills 与 Agents 列表。
  - 新增跨平台开机自动启动（macOS LaunchAgent / Windows HKCU Run / Linux XDG Autostart）及开机轻量启动开关；支持无主 WebView 启动并在 Rust 侧自动恢复共享 MCP 服务。
- Out of Scope:
  - 在应用内编辑或安装/卸载 `SKILL.md` 文件内容。
  - 修改 MCP `compact` 工具档位暴露的 21 个工具集合数量契约。
  - 第三方付费签名或 macOS `SMAppService` 登录项重构（保留对自签名 `.app` 友好的用户态 LaunchAgent 方案）。

## 需求列表

| FR ID | 需求摘要 | 主子规格 |
|---|---|---|
| FR-1 | 修复 Skills/Instructions 扫描：跟随软链目录、解析 YAML 多行块标量与 CRLF、安全截断长描述并覆盖 `~/.agents/skills` 及 Antigravity/Gemini 标准目录 | skills-scan-and-filter |
| FR-2 | 全局与工作区扫描接口返回完整去重的 Skills 与 Instructions 列表（含稳定 ID、来源、作用域、路径与描述），并保留已禁用条目以便界面重新勾选 | skills-scan-and-filter |
| FR-3 | 持久化全局与工作区级 `disabled_skill_ids` / `disabled_instruction_ids`，并在 MCP 指令渲染、`list_skills`、`get_skill` 与 `workspace_select` 中统一过滤 | skills-scan-and-filter |
| FR-4 | 全局设置「AI 指令与技能来源」默认自动扫描并直接展示可勾选注入的 Skills 与 Agents 列表（搜索/筛选/全选/取消），默认折叠 Harness 来源与自定义路径 | skills-picker-ui |
| FR-5 | 工作区设置「AI 指令与技能上下文」默认自动扫描并直接展示生效的 Skills 与 Agents 勾选列表（标记全局禁用状态），默认折叠 Harness 来源与自定义路径 | skills-picker-ui |
| FR-6 | 实现跨平台开机自动启动注册与注销原语（macOS LaunchAgent、Windows HKCU Run、Linux XDG Autostart），启动参数携带 `--autostart` | autostart-and-lightweight-launch |
| FR-7 | 实现开机轻量启动运行时生命周期：macOS 零 WebView + Accessory 菜单栏驻留、非 macOS 静默托盘启动，且在 Rust `.setup()` 幂等恢复共享 MCP 服务并支持 Reopen/托盘恢复主窗口 | autostart-and-lightweight-launch |
| FR-8 | 在「应用设置 → 外观与启动 → 启动行为」新增「开机自动启动」与「开机时轻量启动」开关并持久化同步至操作系统启动项 | autostart-and-lightweight-launch |

> 母规格维护 FR 注册与跨模块契约；每个子规格必须为其负责的 FR 编写 EARS 验收标准。

## 非功能需求

- 兼容性：新增设置字段均带 `#[serde(default)]`，旧版本 `profiles.json` 无损升级，不破坏现有单测与 MCP 契约。
- 性能与内存：轻量启动在 macOS 上不得预先构造主 WebView，避免产生无用 WebKit 渲染进程；目录遍历保持 `max_depth` 上限与符号链接防环保护。
- 跨平台无警告：所有平台分支代码在 `cargo clippy --all-targets` 与 `vue-tsc --noEmit` 下零警告、零错误。
- 错误透明：注册/注销开机自启失败时显式返回可读错误，不静默吞错或误持久化为已开启。

## 依赖关系

- 见 `spec-manifest.json`。
