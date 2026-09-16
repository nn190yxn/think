import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    css: false,
    // 本环境 CPU 与内存紧张时，jsdom 冷启动会让首个用例显著变慢；放宽上限避免误报超时。
    testTimeout: 15000,
  },
});
