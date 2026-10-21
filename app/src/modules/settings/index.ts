import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "settings", title: "Paramètres", order: 40, component: View } satisfies UiModule;
