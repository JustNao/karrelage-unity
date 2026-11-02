import { mount } from "svelte";
import "./app.css";
import FightMapOverlay from "./FightMapOverlay.svelte";

mount(FightMapOverlay, { target: document.getElementById("app")! });
