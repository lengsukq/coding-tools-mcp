import { createRouter, createWebHashHistory, type RouteLocationNormalized } from "vue-router";
import { legacySettingsSection } from "./lib/ui-model";
import { confirmUnsavedChanges } from "./lib/unsaved-changes";

const DashboardView = () => import("./views/DashboardView.vue");
const WorkspaceView = () => import("./views/WorkspaceView.vue");
const SettingsView = () => import("./views/settings/SettingsView.vue");
const ToolAuditView = () => import("./views/ToolAuditView.vue");

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "dashboard", component: DashboardView },
    { path: "/workspace/:id/services", redirect: (to) => `/workspace/${to.params.id}/settings` },
    { path: "/workspace/:id/:tab(overview|planning|changes|history|diagnostics|settings)?", name: "workspace", component: WorkspaceView },
    { path: "/audit", name: "tool-audit", component: ToolAuditView },
    { path: "/settings", redirect: "/settings/general" },
    ...["keys", "gateway", "frp", "lan"].map((panel) => ({
      path: `/settings/${panel}`,
      redirect: { path: `/settings/${legacySettingsSection(panel)!.section}`, query: { panel } },
    })),
    {
      path: "/settings/software",
      redirect: { path: `/settings/${legacySettingsSection("software")!.section}`, query: { panel: "software" } },
    },
    { path: "/settings/:section(general|connection|execution|computer|tools|about)", name: "settings", component: SettingsView },
    ...(import.meta.env.DEV ? [{ path: "/dev/ui", component: () => import("./views/ComponentGalleryView.vue") }] : []),
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});
function isSamePanelRoute(to: RouteLocationNormalized, from: RouteLocationNormalized): boolean {
  if (to.path !== from.path) return false;
  const toPanel = Array.isArray(to.query.panel) ? (to.query.panel[0] ?? "") : (to.query.panel ?? "");
  const fromPanel = Array.isArray(from.query.panel) ? (from.query.panel[0] ?? "") : (from.query.panel ?? "");
  return toPanel === fromPanel;
}

router.beforeEach((to, from) => (isSamePanelRoute(to, from) ? true : confirmUnsavedChanges()));
export default router;
