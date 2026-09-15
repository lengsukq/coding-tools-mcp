import { createRouter, createWebHashHistory } from "vue-router";
import DashboardView from "./views/DashboardView.vue";
import WorkspaceView from "./views/WorkspaceView.vue";
import GeneralSettingsView from "./views/settings/GeneralSettingsView.vue";
import KeysSettingsView from "./views/settings/KeysSettingsView.vue";
import GatewaySettingsView from "./views/settings/GatewaySettingsView.vue";
import FrpSettingsView from "./views/settings/FrpSettingsView.vue";
import SoftwareSettingsView from "./views/settings/SoftwareSettingsView.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "dashboard", component: DashboardView },
    { path: "/workspace/:id", name: "workspace", component: WorkspaceView },
    { path: "/settings/general", name: "settings-general", component: GeneralSettingsView },
    { path: "/settings/keys", name: "settings-keys", component: KeysSettingsView },
    { path: "/settings/gateway", name: "settings-gateway", component: GatewaySettingsView },
    { path: "/settings/frp", name: "settings-frp", component: FrpSettingsView },
    { path: "/settings/software", name: "settings-software", component: SoftwareSettingsView },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});

export default router;
