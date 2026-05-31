import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    // Match the port the preview launch config exposes (3000) so the browser
    // pane points at the right server. Override with VITE_PORT if needed.
    port: Number(process.env.VITE_PORT) || 3000,
    // Dev: forward API + subscription endpoint to the Rust backend.
    proxy: {
      // Trailing slashes are important — without them `/s` would also match
      // `/src/*` (Vite's own dev modules) and proxy them to the Rust server.
      "/api/": "http://127.0.0.1:8080",
      "/sub/": "http://127.0.0.1:8080",
      "/s/":   "http://127.0.0.1:8080",
    },
  },
  build: {
    // Static assets go to /assets/* — matches the Content-Security-Policy
    // we serve from the Rust side.
    assetsDir: "assets",
    chunkSizeWarningLimit: 1024,
  },
});
