import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 在开发时把 devUrl 指向固定端口，该端口不可被自动改写。
const DEV_PORT = 1430;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    host: false,
    // 允许平台预览域名访问开发服务器。
    allowedHosts: [".monkeycode-ai.online"],
    watch: {
      // Rust 侧变更由 tauri dev 负责重建，前端 watcher 不介入。
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
