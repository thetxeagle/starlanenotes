import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const tauriDevHost = process.env.TAURI_DEV_HOST?.trim();

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
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
        target: "http://localhost:3000",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://localhost:3000",
        ws: true,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    // Excalidraw ships as a large prebuilt editor bundle. It is loaded only when
    // a drawing modal opens, so keep the general app chunks split and allow this
    // known async vendor chunk without noisy build warnings.
    chunkSizeWarningLimit: 2500,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("@excalidraw/excalidraw")) {
            return "vendor-excalidraw";
          }
          if (id.includes("@tiptap/") || id.includes("@prosemirror/") || id.includes("prosemirror-") || id.includes("lowlight")) {
            return "vendor-editor";
          }
          if (id.includes("react/") || id.includes("react-dom/") || id.includes("react-router")) {
            return "vendor-react";
          }
          if (id.includes("lucide-react/") || id.includes("@radix-ui/")) {
            return "vendor-ui";
          }
        },
      },
    },
  },
});
