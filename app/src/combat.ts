import { mount } from "svelte";
import "./app.css";
import CombatOverlay from "./CombatOverlay.svelte";

mount(CombatOverlay, { target: document.getElementById("app")! });
