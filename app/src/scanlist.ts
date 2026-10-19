import { mount } from "svelte";
import "./app.css";
import ScanList from "./ScanList.svelte";

mount(ScanList, { target: document.getElementById("app")! });
