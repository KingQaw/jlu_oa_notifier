import { defineConfig } from "vite";

// Tauri CLI 在真机/局域网调试时会注入 TAURI_DEV_HOST（指向开发机 IP）；
// 模拟器与桌面端则为空，走默认的 localhost。
const host = process.env.TAURI_DEV_HOST;
const mobile =
  process.env.TAURI_ENV_PLATFORM === "android" ||
  process.env.TAURI_ENV_PLATFORM === "ios";

export default defineConfig({
  // Tauri 使用固定端口，避免端口漂移。
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // Rust 侧文件变化由 Tauri 自己处理，不需要前端整页刷新
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: mobile ? ["es2021", "chrome105"] : ["es2021", "safari13"],
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
