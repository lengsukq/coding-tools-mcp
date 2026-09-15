import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import "./app.css";
import "./styles/workbench.css";
import "./styles/ios-vue.css";

createApp(App).use(router).mount("#app");
