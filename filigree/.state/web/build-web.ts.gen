#!/usr/bin/env bun
import { $ } from "bun";
import { parseArgs } from "util";

const args = parseArgs({
  options: {
    dev: {
      type: "boolean",
    },
  },
});

await $`rm -rf build/*`.nothrow().quiet();

await $`NODE_ENV=production vite build`;

if (args.values.dev) {
  process.exit(0);
}

let glob = new Bun.Glob("build/**/*.{js,css,js.map}");
let files = Array.from(glob.scanSync());

console.log("Compressing assets...");
await Promise.all(
  files.map(async (path) => {
    let file = await Bun.file(path).text();
    let zipped = Bun.gzipSync(file, { level: 9 });
    await Bun.write(path + ".gz", zipped);
    await $`brotli -s ${path}`;
    console.log(`Compressed ${path}`);
  }),
);
