import type { PlanningStateDto, PlanDto } from "./api/planning";
export type StatusTone = "neutral" | "info" | "success" | "warning" | "danger";
const labels: Record<string, string> = {
    running: "运行中", executing: "执行中", in_progress: "执行中", starting: "启动中", stopping: "停止中",
    stopped: "已停止", idle: "空闲", unknown: "未知", checking: "检查中", healthy: "正常",
    ok: "正常", success: "成功", error: "异常", failed: "失败", blocked: "阻塞",
    pending: "待开始", active: "进行中", paused: "已暂停", draft: "草稿", completed: "已完成",
    awaiting_acceptance: "待验收", archived: "已归档", cancelled: "已取消", skipped: "已跳过", warning: "需关注",
    verified: "已有验证记录", stale: "数据过期", direct: "直接执行", plan: "按计划执行", goal: "按目标执行",
};
export function statusPresentation(value?: string): {
    label: string;
    tone: StatusTone;
} {
    const status = value?.toLowerCase() ?? "unknown";
    const tone: StatusTone = ["error", "failed", "blocked", "danger"].includes(status) ? "danger"
        : ["warning", "awaiting_acceptance", "paused"].includes(status) ? "warning"
            : ["completed", "ok", "success", "healthy"].includes(status) ? "success"
                : ["running", "executing", "in_progress", "starting", "stopping", "active", "checking"].includes(status) ? "info" : "neutral";
    return { label: labels[status] ?? "未知状态", tone };
}
export function planProgress(plan: PlanDto | null | undefined) {
    const steps = plan?.steps ?? [];
    const completed = steps.filter(s => s.status === "completed").length;
    const skipped = steps.filter(s => s.status === "skipped").length;
    return { completed, skipped, total: steps.length, percent: steps.length ? Math.round((completed + skipped) / steps.length * 100) : 0,
        label: steps.length ? `已处理 ${completed + skipped}/${steps.length} · 完成 ${completed} · 跳过 ${skipped}` : "未定义步骤" };
}
export function taskFocus(state: PlanningStateDto | null | undefined) {
    const plan = state?.plans.find(p => p.id === state.focus_plan_id) ?? null;
    const goal = state?.goals.find(g => g.id === (plan ? plan.goal_id : state.focus_goal_id)) ?? null;
    const step = plan?.steps.find(s => state?.execution.plan_id===plan.id && s.id === state.execution.step_id) ?? plan?.steps.find(s => s.status === "in_progress") ?? null;
    return { plan, goal, step, title: plan?.title ?? goal?.title ?? "暂无当前任务", detail: plan && goal ? goal.title : plan?.objective ?? goal?.objective ?? "通过 AI 会话发起目标或计划后，会显示在这里。", progress: planProgress(plan) };
}
export const WORKSPACE_NAV = [
    { value: "overview", label: "概览" }, { value: "planning", label: "任务" }, { value: "changes", label: "变更" },
    { value: "history", label: "历史" }, { value: "diagnostics", label: "诊断" }, { value: "settings", label: "设置" },
] as const;
export const SETTINGS_NAV = [
    { value: "general", label: "通用" }, { value: "connection", label: "MCP 连接" }, { value: "execution", label: "AI 执行" },
    { value: "tools", label: "网络与工具" }, { value: "about", label: "关于" },
] as const;
export function legacySettingsSection(section: string) {
    if (["gateway", "keys", "frp"].includes(section))
        return { section: "connection", panel: section };
    if (section === "software")
        return { section: "tools", panel: "software" };
    return null;
}

/** Disabled access and missing measurements are not successful connectivity checks. */
export function connectionHealthPresentation(item: { ok: boolean; detail: string }) {
    if (item.detail.includes("未启用")) return { status: "stopped", label: "未启用" };
    if (item.detail.includes("未检查")) return { status: "unknown", label: "未检查" };
    return { status: item.ok ? "success" : "error", label: item.ok ? "正常" : "失败" };
}
