import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // splash.html: the frameless loading window shown while the main window loads hidden; scanlist.html: the floating "items to scan" window; combat.html: the floating fight window; fightmap.html: the in-game line of sight; forgemagie.html: the rune chances over the forgemagie window; shoplist.html: the floating "to buy" list (Crafting « XP métiers »)
  build: { rollupOptions: { input: { main: "index.html", splash: "splash.html", scanlist: "scanlist.html", combat: "combat.html", fightmap: "fightmap.html", forgemagie: "forgemagie.html", shoplist: "shoplist.html" } } },
  server: { port: 1421, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
});
