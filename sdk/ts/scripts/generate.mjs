//! Regenerate the committed TypeScript artifacts from the contract.
//!
//! Usage (from `sdk/ts`):
//!
//! ```sh
//! npm run generate           # write the artifacts
//! node scripts/generate.mjs --check   # fail if they are stale
//! ```
//!
//! The contract is `docs/api/openapi-v1.json` at the repository root. Passing
//! `--spec <path>` overrides it; the drift test uses that to prove generation
//! is a pure function of the document.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { render } from "./lib/contract.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const repositoryRoot = resolve(packageRoot, "../..");

/** Parse `--spec <path>` and `--check`. */
function parseArgs(argv) {
  const args = { check: false, spec: join(repositoryRoot, "docs/api/openapi-v1.json") };
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--check") args.check = true;
    else if (argv[index] === "--spec") args.spec = resolve(argv[++index]);
    else throw new Error(`unknown argument: ${argv[index]}`);
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
const spec = JSON.parse(readFileSync(args.spec, "utf8"));
const artifacts = render(spec);
const stale = [];

for (const [path, contents] of Object.entries(artifacts)) {
  if (path === "routes") continue;
  const target = join(packageRoot, path);
  let current = "";
  try {
    current = readFileSync(target, "utf8");
  } catch {
    current = "";
  }
  if (args.check) {
    if (current !== contents) {
      stale.push(relative(packageRoot, target));
    }
    continue;
  }
  if (current === contents) continue;
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, contents);
  process.stdout.write(`wrote ${relative(packageRoot, target)}\n`);
}

if (args.check) {
  if (stale.length) {
    process.stderr.write(
      `generated artifacts are stale:\n${stale.map((path) => `  - ${path}`).join("\n")}\n` +
        "run `npm run generate` from sdk/ts and commit the result\n",
    );
    process.exit(1);
  }
  process.stdout.write(`generated artifacts are current (${artifacts.routes.length} routes)\n`);
}
