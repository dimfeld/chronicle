import daisyui from "daisyui";
import { fontFamily } from "tailwindcss/defaultTheme";

/** @type {import('tailwindcss').Config} */
const config = {
  darkMode: ["class"],
  content: [
    "./**/src/*.{html,js,svelte,ts}",
    "../src/pages/**/*.{rs,js,html,ts}",
  ],
  plugins: [daisyui],
  safelist: ["dark", "fill-current"],
  theme: {
    extend: {
      fontFamily: {
        sans: [...fontFamily.sans],
      },
    },
  },
  daisyui: {
    logs: false,
  },
};

export default config;
