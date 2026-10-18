import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "auction", title: "Items", order: 10, component: View } satisfies UiModule;
