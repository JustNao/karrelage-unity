import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "stuff", title: "Stuff", order: 16, component: View } satisfies UiModule;
