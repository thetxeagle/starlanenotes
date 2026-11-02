import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import { readFileSync } from "node:fs";

const packageVersion = JSON.parse(
  readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
).version as string;

const tauriDevHost = process.env.TAURI_DEV_HOST?.trim();
const apiProxyTarget = process.env.STARLANE_API_PROXY_TARGET?.trim() || "http://localhost:3000";
const websocketProxyTarget = apiProxyTarget.replace(/^http/i, "ws");
const shouldInlineBuildAsset = (filePath: string): boolean | undefined =>
  /\.(?:woff2?|ttf|otf)$/i.test(filePath) ? false : undefined;

export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],
  define: {
    __STARLANE_VERSION__: JSON.stringify(packageVersion),
  },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  server: {
    host: tauriDevHost ? "0.0.0.0" : undefined,
    port: 5173,
    strictPort: Boolean(tauriDevHost),
    hmr: tauriDevHost
      ? {
          host: tauriDevHost,
          protocol: "ws",
        }
      : undefined,
    proxy: {
      "/api": {
        target: apiProxyTarget,
        changeOrigin: true,
      },
      "/ws": {
        target: websocketProxyTarget,
        ws: true,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    chunkSizeWarningLimit: 2500,
    assetsInlineLimit: shouldInlineBuildAsset,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "vendor-editor-syntax",
              test: (id) => id.includes("highlight.js") || id.includes("/editorSyntax"),
            },
            {
              name: "vendor-editor",
              test: (id) =>
                id.includes("@tiptap/") ||
                id.includes("@prosemirror/") ||
                id.includes("prosemirror-") ||
                id.includes("lowlight"),
            },
            {
              name: "vendor-react",
              test: (id) =>
                id.includes("react/") || id.includes("react-dom/") || id.includes("react-router"),
            },
            {
              name: "vendor-ui",
              test: (id) => id.includes("lucide-react/") || id.includes("@radix-ui/"),
            },
          ],
        },
      },
    },
  },
}));
