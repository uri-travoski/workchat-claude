import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });

// Surface unexpected script errors instead of failing silently.
window.addEventListener("unhandledrejection", (e) => console.error("unhandled rejection", e.reason));
window.addEventListener("error", (e) => console.error("script error", e.message));
