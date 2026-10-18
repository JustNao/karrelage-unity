import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // splash.html: the frameless loading window shown while the main window loads hidden
  build: { rollupOptions: { input: { main: "index.html", splash: "splash.html" } } },
  server: { port: 1421, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
});
