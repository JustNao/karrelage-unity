import View from "./View.svelte";
import type { CraftSub } from "../../subs";

export default { id: "rentables", title: "Crafts rentables", description: "Coût de craft et prix de vente de chaque objet craftable", order: 12, component: View } satisfies CraftSub;
