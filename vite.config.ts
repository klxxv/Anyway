import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": projectRoot,
    },
  },
  optimizeDeps: {
    // Keep the renderer's stable Vue runtime and graph dependencies in one
    // pre-bundled dependency graph so cold starts do not rediscover them from
    // a deep SFC import waterfall.
    include: [
      "vue",
      "pinia",
      "@vue-flow/core",
      "@vue-flow/background",
      "@vue-flow/controls",
      "@vue-flow/minimap",
      "@tabler/icons-vue",
      "jspdf",
    ],
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/target/**",
        "**/plugins/installed/**",
        "**/plugins/packages/**",
        "**/build/**",
        "**/crates/**",
        "**/.next/**",
        "**/.wrangler/**",
        "**/.vinext/**",
        "**/out/**",
        "**/output/**",
        "**/dist/**",
      ],
    },
    warmup: {
      clientFiles: [
        "./src/main.ts",
        "./src/App.vue",
        "./src/vue/ResearchWorkspaceApp.vue",
        "./src/vue/canvas/ResearchGraphCanvas.vue",
        "./src/vue/components/WorkspaceTopbar.vue",
        "./src/vue/components/InspectorPanel.vue",
      ],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
