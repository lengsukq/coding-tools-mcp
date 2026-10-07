import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import { initPalette } from "./lib/palette";
import "./app.css";
import "./palette.css";
import "./styles/design-system.css";

initPalette();

createApp(App).use(router).mount("#app");
