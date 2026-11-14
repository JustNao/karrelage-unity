import { mount } from "svelte";
import "./app.css";
import ShopList from "./ShopList.svelte";

mount(ShopList, { target: document.getElementById("app")! });
