import { createApp } from "vue";
import { pinia } from "./vue/stores/pinia";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "@vue-flow/minimap/dist/style.css";
import "../app/globals.css";
import "./vue/canvas/vue-flow-compat.css";
import App from "./App.vue";

createApp(App).use(pinia).mount("#app");
