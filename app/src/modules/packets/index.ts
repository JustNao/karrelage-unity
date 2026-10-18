import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "packets", title: "Packets", order: 20, devOnly: true, component: View } satisfies UiModule;
