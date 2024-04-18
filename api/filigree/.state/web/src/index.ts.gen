import "./app.postcss";
import { startLiveReload } from "./livereload.js";

import Alpine from "alpinejs";
import morph from "@alpinejs/morph";

import htmx from "./htmx.js";
import "htmx.org/dist/ext/alpine-morph.js";
import "htmx.org/dist/ext/head-support.js";

// Add `checked` to default list so that DaisyUI toggle checkboxes will animate across page load
htmx.config.attributesToSettle = [
  "class",
  "style",
  "width",
  "height",
  "checked",
];

window.Alpine = Alpine;
Alpine.plugin(morph);
Alpine.start();

if (process.env.LIVE_RELOAD === "true") {
  startLiveReload();
}
