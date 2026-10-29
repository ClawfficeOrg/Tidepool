//! Contract gates for the SDK.
//!
//! - the committed artifacts are byte-identical to a fresh generation;
//! - every route in `docs/api/openapi-v1.json` has a typed method, and calling
//!   it dispatches the matching operation id;
//! - every hub route carries at least one documented error, and every
//!   documented 429 is marked retryable;
//! - the scope list matches the contract.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { render } from "../scripts/lib/contract.mjs";
import { buildResources } from "../src/generated/resources.js";
import { CONTRACT_VERSION, ROLES, ROUTES, SCOPES } from "../src/generated/routes.js";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const specPath = join(packageRoot, "../../docs/api/openapi-v1.json");
const spec = JSON.parse(readFileSync(specPath, "utf8"));

/** Every `(method, path, operationId)` in the contract. */
function contractOperations() {
  const operations = [];
  for (const [path, entries] of Object.entries(spec.paths)) {
    for (const [method, operation] of Object.entries(entries)) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) continue;
      operations.push({ method: method.toUpperCase(), path, operationId: operation.operationId });
    }
  }
  return operations;
}

test("committed artifacts match a fresh generation", () => {
  const artifacts = render(spec);
  const stale = [];
  for (const [path, contents] of Object.entries(artifacts)) {
    if (path === "routes") continue;
    const committed = readFileSync(join(packageRoot, path), "utf8");
    if (committed !== contents) stale.push(path);
  }
  assert.deepEqual(stale, [], `stale generated artifacts: ${stale.join(", ")} (run npm run generate)`);
});

test("every contract route has a route table entry", () => {
  const operations = contractOperations();
  assert.ok(operations.length > 80, `expected a full contract, saw ${operations.length} operations`);
  for (const operation of operations) {
    const route = ROUTES[operation.operationId];
    assert.ok(route, `missing route table entry for ${operation.operationId}`);
    assert.equal(route.method, operation.method);
    assert.equal(route.path, operation.path);
  }
  assert.equal(Object.keys(ROUTES).length, operations.length, "route table has stray entries");
});

test("every contract route has a typed method that dispatches its operation id", () => {
  const calls = [];
  const client = {
    _call(operationId, options, pathParams, body) {
      calls.push({ operationId, options, pathParams, body, kind: "call" });
      return Promise.resolve(undefined);
    },
    _stream(operationId, options, pathParams) {
      calls.push({ operationId, options, pathParams, kind: "stream" });
      return { closed: Promise.resolve() };
    },
  };
  const resources = buildResources(client);

  for (const route of Object.values(ROUTES)) {
    let node = resources;
    for (const segment of route.namespace) {
      assert.ok(node[segment], `missing namespace ${route.namespace.join(".")} for ${route.operationId}`);
      node = node[segment];
    }
    const method = node[route.name];
    assert.equal(typeof method, "function", `missing method ${route.namespace.join(".")}.${route.name}`);

    const args = [];
    if (route.pathParams.length === 1) args.push("path-value");
    if (route.pathParams.length > 1) args.push(Object.fromEntries(route.pathParams.map((name) => [name, "x"])));
    if (route.body) args.push({});
    args.push({ requestId: "probe" });

    method(...args);
    const call = calls.at(-1);
    assert.equal(call.operationId, route.operationId, `method ${route.name} dispatched ${call.operationId}`);
    assert.equal(call.kind, route.response === "ChatStream" ? "stream" : "call");
    for (const name of route.pathParams) {
      const recorded =
        call.pathParams && typeof call.pathParams === "object" ? call.pathParams[name] : call.pathParams;
      assert.equal(recorded, route.pathParams.length === 1 ? "path-value" : "x");
    }
    assert.equal(call.options.requestId, "probe");
  }
  assert.equal(calls.length, Object.keys(ROUTES).length);
});

test("a generated typecheck call exists for every route", () => {
  const conformance = readFileSync(join(packageRoot, "test/generated/conformance.ts"), "utf8");
  for (const operationId of Object.keys(ROUTES)) {
    assert.match(conformance, new RegExp(`const ${operationId}: `), `no typed call for ${operationId}`);
  }
});

test("every hub route documents at least one typed error", () => {
  for (const [operationId, route] of Object.entries(ROUTES)) {
    if (route.surface !== "hub") continue;
    assert.ok(
      route.errors.length > 0 || route.rateLimited,
      `${operationId} documents no typed error response`,
    );
    assert.equal(route.rateLimited, true, `${operationId} does not document 429`);
    assert.ok(
      ["public", "member", "member_or_api_key", "api_key", "host", "signature"].includes(route.auth),
      `${operationId} has an unknown auth model ${route.auth}`,
    );
  }
});

test("scopes and roles match the contract", () => {
  assert.deepEqual([...SCOPES], [
    "projects:read",
    "configuration:validate",
    "configuration:install",
    "runs:dispatch",
    "daemons:enroll",
  ]);
  assert.deepEqual([...ROLES], ["owner", "admin", "member", "viewer"]);
  const declared = new Set();
  for (const entries of Object.values(spec.paths)) {
    for (const operation of Object.values(entries)) {
      if (operation["x-required-scope"]) declared.add(operation["x-required-scope"]);
    }
  }
  for (const scope of declared) {
    assert.ok(SCOPES.includes(scope), `contract declares an undeclared scope ${scope}`);
  }
  assert.equal(CONTRACT_VERSION, spec.info.version);
});

test("problem codes are typed and tolerate unknown codes", () => {
  const types = readFileSync(join(packageRoot, "src/generated/types.d.ts"), "utf8");
  const codes = spec.components.schemas.Problem.properties.code.enum;
  for (const code of codes) {
    assert.ok(types.includes(JSON.stringify(code)), `ProblemCode is missing ${code}`);
  }
  assert.match(types, /export type ProblemCode = [^;]*\(string & \{\}\)/);
  assert.match(types, /code: ProblemCode;/);
});
