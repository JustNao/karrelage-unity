import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "prices", title: "Stored prices", order: 15, component: View } satisfies UiModule;
