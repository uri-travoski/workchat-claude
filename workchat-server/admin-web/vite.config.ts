import { defineConfig } from "vite";

// The server serves the built files at /admin/.
export default defineConfig({
  base: "/admin/",
  build: { outDir: "dist", emptyOutDir: true, target: "es2020" },
  server: { proxy: { "/api": "http://127.0.0.1:18300" } },
});
