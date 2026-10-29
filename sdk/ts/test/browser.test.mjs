//! The browser build must not reach a Node-only module.
//!
//! The test walks every static `import`/`export ... from` reachable from
//! `src/browser.js`, resolves relative specifiers, and fails on a `node:`
//! prefix, a bare package specifier, or a missing file. It also checks the
//! package's conditional exports so `browser` wins over `node`.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");

/** Static specifiers in one module. */
function specifiersOf(source) {
  const specifiers = new Set();
  const patterns = [
    /(?:^|\n)\s*import\s+[^'"]*?from\s*["']([^"']+)["']/g,
    /(?:^|\n)\s*export\s+[^'"]*?from\s*["']([^"']+)["']/g,
    /(?:^|\n)\s*import\s*["']([^"']+)["']/g,
  ];
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern)) specifiers.add(match[1]);
  }
  return [...specifiers];
}

/** Resolve an ESM relative specifier to a real file. */
function resolveRelative(base, specifier) {
  const target = resolve(dirname(base), specifier);
  const candidates = [target, target.replace(/\.js$/, ".d.ts"), target.replace(/\.js$/, "/index.js")];
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

/** Every module reachable from `entry`, with its specifiers. */
function importGraph(entry) {
  const seen = new Map();
  const queue = [resolve(packageRoot, entry)];
  while (queue.length) {
    const file = queue.pop();
    if (seen.has(file)) continue;
    const source = readFileSync(file, "utf8");
    const specifiers = specifiersOf(source);
    seen.set(file, specifiers);
    for (const specifier of specifiers) {
      if (!specifier.startsWith(".")) continue;
      const resolved = resolveRelative(file, specifier);
      if (resolved) queue.push(resolved);
    }
  }
  return seen;
}

test("the browser entry reaches no Node-only or bare module", () => {
  const graph = importGraph("src/browser.js");
  assert.ok(graph.size > 5, `expected a real import graph, saw ${graph.size} modules`);
  const problems = [];
  for (const [file, specifiers] of graph) {
    for (const specifier of specifiers) {
      if (specifier.startsWith("node:")) problems.push(`${file} imports ${specifier}`);
      else if (specifier.startsWith(".")) {
        if (!resolveRelative(file, specifier)) problems.push(`${file} imports missing ${specifier}`);
      } else {
        problems.push(`${file} imports bare specifier ${specifier}`);
      }
    }
    const source = readFileSync(file, "utf8");
    if (/\brequire\s*\(/.test(source)) problems.push(`${file} calls require()`);
    if (/\bprocess\s*\./.test(source)) problems.push(`${file} reads process`);
    if (/\bBuffer\b/.test(source)) problems.push(`${file} references Buffer`);
  }
  assert.deepEqual(problems, []);
});

test("the Node entry is the only module allowed to import node:fs", () => {
  const graph = importGraph("src/node.js");
  const consumers = [...graph.entries()]
    .filter(([, specifiers]) => specifiers.some((specifier) => specifier.startsWith("node:")))
    .map(([file]) => file.replace(`${packageRoot}/`, ""));
  assert.deepEqual(consumers, ["src/node.js"]);
});

test("package exports pick the browser build before the Node build", () => {
  const manifest = JSON.parse(readFileSync(resolve(packageRoot, "package.json"), "utf8"));
  const conditions = Object.keys(manifest.exports["."]);
  assert.equal(conditions[0], "types");
  assert.ok(conditions.includes("browser"));
  assert.ok(conditions.includes("node"));
  assert.ok(conditions.indexOf("browser") < conditions.indexOf("node"), "browser must win in a bundler");
  assert.equal(manifest.exports["."].browser, "./src/browser.js");
  assert.equal(manifest.exports["."].node, "./src/node.js");
  assert.equal(manifest.type, "module");
  assert.equal(manifest.name, "@clawffice/tidepool-sdk");
  assert.equal(manifest.publishConfig.access, "public");
});
