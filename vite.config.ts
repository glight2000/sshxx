import { execSync } from "node:child_process";

import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const commitHash = execSync("git rev-parse --short HEAD").toString().trim();
const tauriDevHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify("0.11.0-" + commitHash),
  },

  plugins: [sveltekit()],

  server: {
    // LAN-reachable development UI; the daemon/server remain loopback-only.
    // Browser encryption still requires HTTPS or a localhost SSH tunnel.
    host: tauriDevHost || "0.0.0.0",
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ["**/target/**", "**/build/**"],
    },
    hmr: tauriDevHost
      ? {
          protocol: "ws",
          host: tauriDevHost,
          port: 5174,
        }
      : undefined,
    proxy: {
      "/api": {
        target: "http://[::1]:8051",
        changeOrigin: true,
        ws: true,
      },
    },
  },
});
