#!/usr/bin/env node
// Fails if any source file outside the theme token sheet contains a hard-coded
// colour. All colours must come from CSS variables in src/styles/tokens.css so
// that both themes stay consistent (Issue #1106).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("../src", import.meta.url).pathname;
const allowed = new Set(["styles/tokens.css"]);
const patterns = [
  { name: "hex colour", re: /#[0-9a-fA-F]{3,8}\b(?![-\w])/g },
  { name: "rgb()/hsl() colour", re: /\b(?:rgba?|hsla?|oklch|lab)\(/g },
  { name: "named colour", re: /:\s*(?:white|black|red|green|blue|gray|grey|orange|yellow)\s*[;!}]/g },
];

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

let failures = 0;
for (const file of walk(root)) {
  const rel = relative(root, file);
  if (allowed.has(rel) || !/\.(css|tsx?)$/.test(file)) continue;
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((raw, i) => {
      // Ignore comments so references like "Issue #1105" are not flagged.
      if (/^\s*(\/\/|\/?\*)/.test(raw)) return;
      const line = raw.replace(/\/\/.*$|\/\*.*?\*\//g, "");
      for (const { name, re } of patterns) {
        re.lastIndex = 0;
        if (re.test(line)) {
          failures++;
          console.error(`src/${rel}:${i + 1}: hard-coded ${name}: ${raw.trim()}`);
        }
      }
    });
}

if (failures) {
  console.error(`\n${failures} hard-coded colour(s) found. Use a token from src/styles/tokens.css.`);
  process.exit(1);
}
console.log("No hard-coded colours found.");
