import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

const root = import.meta.dirname;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        control: resolve(root, "index.html"),
        stage: resolve(root, "stage.html"),
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
