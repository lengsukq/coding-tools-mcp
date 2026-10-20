import { createRouter, createWebHashHistory } from "vue-router";
import DashboardView from "./views/DashboardView.vue";
import WorkspaceView from "./views/WorkspaceView.vue";
import SettingsView from "./views/settings/SettingsView.vue";
import ToolAuditView from "./views/ToolAuditView.vue";
import { legacySettingsSection } from "./lib/ui-model";
import { confirmUnsavedChanges } from "./lib/unsaved-changes";
const router = createRouter({ history: createWebHashHistory(), routes: [
        { path: "/", name: "dashboard", component: DashboardView },
        { path: "/workspace/:id/services", redirect: to => `/workspace/${to.params.id}/settings` },
        { path: "/workspace/:id/:tab(overview|planning|changes|history|diagnostics|settings)?", name: "workspace", component: WorkspaceView },
        { path: "/audit", name: "tool-audit", component: ToolAuditView },
        { path: "/settings", redirect: "/settings/general" },
        ...["keys", "gateway", "frp"].map(panel => ({ path: `/settings/${panel}`, redirect: { path: `/settings/${legacySettingsSection(panel)!.section}`, query: { panel } } })),
        { path: "/settings/software", redirect: { path: `/settings/${legacySettingsSection("software")!.section}`, query: { panel: "software" } } },
        { path: "/settings/:section(general|connection|execution|computer|tools|about)", name: "settings", component: SettingsView },
        ...(import.meta.env.DEV ? [{ path: "/dev/ui", component: () => import("./views/ComponentGalleryView.vue") }] : []),
        { path: "/:pathMatch(.*)*", redirect: "/" },
    ] });
router.beforeEach((to, from) => to.path === from.path ? true : confirmUnsavedChanges());
export default router;
