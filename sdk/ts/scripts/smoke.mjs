//! Publish smoke test for `@clawffice/tidepool-sdk`.
//!
//! 1. `npm pack` the package (no registry, no network).
//! 2. Install the tarball into a fresh temporary project.
//! 3. Import the installed package by name and prove a fresh install can mint
//!    an enrollment token, enrol a host and dispatch a run. The default run is
//!    hermetic: the installed package talks to a recorded transport built from
//!    `test/fixtures/enrol-and-dispatch.json`, so no server or database is
//!    required.
//!
//! For the dev stack, pass `--live <baseUrl>` (and `TIDEPOOL_TOKEN`, plus
//! `TIDEPOOL_PROJECT` and `TIDEPOOL_TRIGGER` for dispatch). Credentials are
//! read from the environment by name and never printed.
//!
//! Usage, from `sdk/ts`:
//!
//! ```sh
//! npm run smoke
//! node scripts/smoke.mjs --live http://localhost:8080
//! ```

import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");

/** Parse `--live <url>` and `--keep`. */
function parseArgs(argv) {
  const args = { live: null, keep: false };
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--live") args.live = argv[++index];
    else if (argv[index] === "--keep") args.keep = true;
    else throw new Error(`unknown argument: ${argv[index]}`);
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
const workspace = mkdtempSync(join(tmpdir(), "tidepool-sdk-smoke-"));
const consumer = join(workspace, "consumer");
mkdirSync(consumer, { recursive: true });

try {
  process.stdout.write("packing the package\n");
  const packed = execFileSync("npm", ["pack", "--json", "--pack-destination", workspace], {
    cwd: packageRoot,
    encoding: "utf8",
  });
  const [{ filename, files }] = JSON.parse(packed);
  const tarball = join(workspace, filename);
  const names = files.map((file) => file.path);
  for (const required of [
    "package.json",
    "src/index.js",
    "src/index.d.ts",
    "src/browser.js",
    "src/node.js",
    "src/client.js",
    "src/generated/routes.js",
    "src/generated/types.d.ts",
  ]) {
    if (!names.includes(required)) throw new Error(`packed tarball is missing ${required}`);
  }
  process.stdout.write(`packed ${filename} with ${names.length} files\n`);

  process.stdout.write("installing the tarball into a fresh project\n");
  writeFileSync(join(consumer, "package.json"), JSON.stringify({ name: "smoke-consumer", private: true }, null, 2));
  execFileSync(
    "npm",
    ["install", "--no-save", "--no-audit", "--no-fund", "--ignore-scripts", "--offline", tarball],
    { cwd: consumer, stdio: "inherit" },
  );

  const steps = JSON.parse(
    readFileSync(join(packageRoot, "test/fixtures/enrol-and-dispatch.json"), "utf8"),
  ).steps;
  const runner = args.live ? liveRunner(args.live) : recordedRunner(steps);
  writeFileSync(join(consumer, "smoke.mjs"), runner);

  process.stdout.write(
    args.live ? `running the fresh install against ${args.live}\n` : "running the fresh install hermetically\n",
  );
  const output = execFileSync("node", ["smoke.mjs"], { cwd: consumer, encoding: "utf8" });
  process.stdout.write(output);

  process.stdout.write("checking the browser export condition\n");
  execFileSync(
    "node",
    [
      "--input-type=module",
      "-e",
      [
        'import("@clawffice/tidepool-sdk/browser").then((browser) => {',
        '  if (typeof browser.createClient !== "function") throw new Error("browser entry is incomplete");',
        '  const client = browser.createClient({ baseUrl: "https://example.invalid", fetch: async () => new Response(null, { status: 204 }) });',
        '  if (typeof client.chatrooms.stream !== "function") throw new Error("browser entry lacks the typed resources");',
        '  console.log("browser entry loads with no Node-only dependency");',
        "});",
      ].join("\n"),
    ],
    { cwd: consumer, stdio: "inherit" },
  );

  process.stdout.write("SMOKE_OK\n");
} finally {
  if (args.keep) process.stdout.write(`kept ${workspace}\n`);
  else rmSync(workspace, { recursive: true, force: true });
}

/** A self-contained runner that replays the recorded enrol/dispatch exchanges. */
function recordedRunner(steps) {
  return `import assert from "node:assert/strict";
import { createClient, deliveryKey, ProblemError } from "@clawffice/tidepool-sdk";

const steps = ${JSON.stringify(steps)};
const calls = [];
let cursor = 0;

const fetch = async (url, init = {}) => {
  const parsed = new URL(String(url));
  const method = (init.method ?? "GET").toUpperCase();
  calls.push({ method, path: parsed.pathname, query: Object.fromEntries(parsed.searchParams), body: init.body ? JSON.parse(init.body) : undefined });
  const step = steps[cursor];
  if (!step) throw new Error("ran out of recorded exchanges");
  if (step.request.method !== method || step.request.path !== parsed.pathname) {
    throw new Error(\`expected \${step.request.method} \${step.request.path}, got \${method} \${parsed.pathname}\`);
  }
  if (step.request.body) assert.deepEqual(JSON.parse(init.body), step.request.body);
  cursor += 1;
  const headers = { ...(step.response.headers ?? {}) };
  const payload = step.response.body === undefined ? null : JSON.stringify(step.response.body);
  if (payload && !headers["content-type"]) headers["content-type"] = "application/json";
  return new Response(payload, { status: step.response.status ?? 200, headers });
};

const operator = createClient({ baseUrl: "https://pool.example.com", token: "operator-jwt", fetch });
const minted = await operator.hosts.mintEnrollmentToken({ host_id: "host_01" });
assert.equal(typeof minted.token, "string");

const host = await operator.hosts.enroll({ token: minted.token, credential: "host-secret" });
assert.equal(host.id, "host_01");
assert.equal(host.status, "connected");

const key = deliveryKey();
const dispatched = await operator.runs.dispatch({
  project: "proj_01",
  trigger: "nightly",
  actor: "operator_01",
  deliveryKey: "smoke-1",
  input: "run the nightly build",
});
assert.equal(dispatched.run.id, "run_01");
assert.equal(dispatched.deduplicated, false);
assert.equal(typeof key, "string");
assert.equal(cursor, steps.length, "every recorded exchange was used");

// A typed error still arrives typed through the installed package.
const failing = createClient({
  baseUrl: "https://pool.example.com",
  fetch: async () => new Response(JSON.stringify({ type: "about:blank", title: "Forbidden", status: 403, code: "forbidden", requestId: "smoke-1" }), {
    status: 403,
    headers: { "content-type": "application/problem+json" },
  }),
});
const problem = await failing.hosts.list().catch((error) => error);
assert.ok(problem instanceof ProblemError);
assert.equal(problem.requestId, "smoke-1");

console.log(\`enrolled \${host.id} and dispatched \${dispatched.run.id} through the published package\`);
`;
}

/** A runner that exercises a live dev stack using environment variables. */
function liveRunner(baseUrl) {
  return `import assert from "node:assert/strict";
import { createClient, deliveryKey } from "@clawffice/tidepool-sdk";

const baseUrl = ${JSON.stringify(baseUrl)};
const token = process.env.TIDEPOOL_TOKEN;
const project = process.env.TIDEPOOL_PROJECT;
const trigger = process.env.TIDEPOOL_TRIGGER;
if (!token) throw new Error("TIDEPOOL_TOKEN is required for --live");
if (!project) throw new Error("TIDEPOOL_PROJECT is required for --live");
if (!trigger) throw new Error("TIDEPOOL_TRIGGER is required for --live");

const operator = createClient({ baseUrl, token });
const created = await operator.hosts.create({
  display_name: "sdk smoke host",
  hostname: "sdk-smoke.local",
});
const minted = await operator.hosts.mintEnrollmentToken({ host_id: created.id });
const credential = deliveryKey();
const host = await operator.hosts.enroll({ token: minted.token, credential });
assert.equal(host.id, created.id);

const dispatched = await operator.runs.dispatch({
  project,
  trigger,
  actor: "sdk-smoke",
  deliveryKey: deliveryKey(),
  input: "sdk smoke dispatch",
});
assert.equal(typeof dispatched.run.id, "string");
console.log(\`enrolled \${host.id} and dispatched \${dispatched.run.id} on the dev stack\`);
`;
}
