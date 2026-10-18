import View from "./View.svelte";
import type { UiModule } from "../../lib/registry";

export default { id: "prices", title: "Prix enregistrés", order: 15, component: View } satisfies UiModule;
