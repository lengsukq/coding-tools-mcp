import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import { initPalette } from "./lib/palette";
import "./app.css";
import "./styles/workbench.css";
import "./styles/ios-vue.css";
import "./palette.css";

initPalette();

createApp(App).use(router).mount("#app");
