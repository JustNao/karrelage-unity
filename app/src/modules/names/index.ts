import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "names", title: "Proto", order: 30, devOnly: true, component: View } satisfies UiModule;
