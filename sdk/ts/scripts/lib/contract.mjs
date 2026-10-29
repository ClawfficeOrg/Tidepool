//! Contract reader and emitter for the TypeScript SDK.
//!
//! `docs/api/openapi-v1.json` is the only input. This module turns it into
//! three committed artifacts plus a type-conformance file, all rendered
//! deterministically so a drift test can regenerate them and compare bytes:
//!
//! - `src/generated/types.d.ts` — one interface per contract schema;
//! - `src/generated/routes.js` — the route table the runtime dispatches on;
//! - `src/generated/resources.js` — one delegating method per contract route;
//! - `src/generated/resources.d.ts` — the typed surface of those methods;
//! - `test/generated/conformance.ts` — typecheck-only call per route.
//!
//! Every route in the document must have an explicit `{group, name}` mapping
//! below. A new route with no mapping fails generation, so the SDK cannot
//! silently miss a contract addition.

/** Operation id to `[namespace path..., method name]`. */
const NAMES = {
  // System (operational; typed for completeness, not a client API).
  get_healthz: ["system", "health"],
  get_metrics: ["system", "metrics"],
  get_openapi_json: ["system", "openapi"],
  get_openapi_v1_json: ["system", "openapiV1"],

  // Setup and pool identity.
  post_v1_setup: ["setup", "create"],
  get_v1_setup_status: ["setup", "status"],
  post_v1_pool_login: ["pool", "login"],
  get_v1_pool_me: ["pool", "me"],
  get_v1_pool_members: ["pool", "members"],
  patch_v1_pool_members_id: ["pool", "updateMember"],
  delete_v1_pool_members_id: ["pool", "removeMember"],
  get_v1_pool_invitations: ["pool", "invitations"],
  post_v1_pool_invitations: ["pool", "invite"],
  post_v1_pool_invitations_accept: ["pool", "acceptInvitation"],
  delete_v1_pool_invitations_id: ["pool", "revokeInvitation"],
  post_v1_pool_password: ["pool", "changePassword"],

  // API keys.
  get_v1_api_keys: ["apiKeys", "list"],
  post_v1_api_keys: ["apiKeys", "create"],
  get_v1_api_keys_whoami: ["apiKeys", "whoami"],
  delete_v1_api_keys_id: ["apiKeys", "remove"],

  // Hosts, enrolment, presence and capabilities.
  get_v1_hosts: ["hosts", "list"],
  post_v1_hosts: ["hosts", "create"],
  post_v1_hosts_enroll: ["hosts", "enroll"],
  post_v1_hosts_enrollment_tokens: ["hosts", "mintEnrollmentToken"],
  get_v1_hosts_id: ["hosts", "get"],
  patch_v1_hosts_id: ["hosts", "rename"],
  delete_v1_hosts_id: ["hosts", "remove"],
  get_v1_hosts_id_capabilities: ["hosts", "capabilities"],
  post_v1_hosts_id_capabilities: ["hosts", "reportCapabilities"],
  post_v1_hosts_id_execution: ["hosts", "startExecution"],
  delete_v1_hosts_id_execution: ["hosts", "stopExecution"],
  post_v1_hosts_id_executions: ["hosts", "createExecution"],

  // Connections.
  get_v1_connections: ["connections", "list"],
  post_v1_connections: ["connections", "create"],
  get_v1_connections_id: ["connections", "get"],
  patch_v1_connections_id: ["connections", "update"],
  delete_v1_connections_id: ["connections", "remove"],

  // Projects, configuration and revisions.
  get_v1_projects: ["projects", "list"],
  post_v1_projects: ["projects", "create"],
  get_v1_projects_id: ["projects", "get"],
  patch_v1_projects_id: ["projects", "rename"],
  delete_v1_projects_id: ["projects", "remove"],
  get_v1_projects_id_configuration: ["projects", "configuration"],
  post_v1_projects_id_configuration_install: ["projects", "installConfiguration"],
  post_v1_projects_id_configuration_validate: ["projects", "validateConfiguration"],
  get_v1_projects_id_revisions: ["projects", "revisions"],
  get_v1_projects_id_revisions_revision_id: ["projects", "revision"],

  // Triggers, workflows and dispatch.
  post_v1_triggers_validate: ["triggers", "validate"],
  post_v1_triggers_install: ["triggers", "install"],
  post_v1_runs: ["runs", "dispatch"],

  // Runs and activity.
  get_v1_activity: ["activity", "list"],
  get_v1_activity_runs_run: ["activity", "run"],

  // Chatrooms: cursor paging plus the live socket.
  get_v1_chatrooms: ["chatrooms", "list"],
  get_v1_chatrooms_room_members: ["chatrooms", "members"],
  get_v1_chatrooms_room_messages: ["chatrooms", "messages"],
  post_v1_chatrooms_room_messages: ["chatrooms", "send"],
  get_v1_chatrooms_room_stream: ["chatrooms", "stream"],

  // Session relay.
  get_v1_sessions_session_relay: ["relay", "get"],
  put_v1_sessions_session_relay: ["relay", "enable"],
  delete_v1_sessions_session_relay: ["relay", "stop"],
  get_v1_sessions_session_relay_events: ["relay", "events"],
  post_v1_sessions_session_relay_events: ["relay", "publish"],

  // Inbound webhooks (provider-signed).
  post_v1_webhooks_github_slug: ["webhooks", "github"],
  post_v1_webhooks_slack_slug: ["webhooks", "slack"],

  // Game surface (Talo-compatible, unchanged by the contract).
  post_v1_auth_login: ["game", "auth", "login"],
  post_v1_auth_register: ["game", "auth", "register"],
  post_v1_events: ["game", "events", "create"],
  post_v1_feedback: ["game", "feedback", "create"],
  get_v1_games_game_id_settings: ["game", "games", "settings"],
  put_v1_games_game_id_settings: ["game", "games", "updateSettings"],
  get_v1_leaderboards_game_id: ["game", "leaderboards", "get"],
  post_v1_leaderboards_game_id_entries: ["game", "leaderboards", "submitEntry"],
  put_v1_leaderboards_game_id_entries_player_id: ["game", "leaderboards", "putEntry"],
  post_v1_players_subaccount: ["game", "players", "createSubaccount"],
  get_v1_players_id: ["game", "players", "get"],
  put_v1_players_id: ["game", "players", "update"],
  delete_v1_players_id: ["game", "players", "remove"],
  get_v1_players_id_subaccounts: ["game", "players", "subaccounts"],
  post_v1_saves: ["game", "saves", "create"],
  get_v1_saves_player_id: ["game", "saves", "list"],
  get_v1_saves_player_id_save_id: ["game", "saves", "get"],
  delete_v1_saves_player_id_save_id: ["game", "saves", "remove"],
  post_v1_socket_tickets: ["game", "socketTickets", "create"],
};

/**
 * Query parameters accepted by handlers but not yet listed in the OpenAPI
 * document. Names and types mirror the Rust `Deserialize` structs; the gap is
 * recorded in `docs/API_CONTRACT.md` §5.1.
 */
const QUERY = {
  get_v1_activity: {
    project: "string",
    provider: "string",
    trigger: "string",
    status: "string",
    kind: "string",
    cursor: "string",
    limit: "number",
  },
  get_v1_chatrooms_room_messages: { after: "number", limit: "number" },
  get_v1_chatrooms_room_stream: { token: "string", after: "number" },
  get_v1_sessions_session_relay_events: { after: "number", limit: "number" },
  get_v1_leaderboards_game_id: { page: "number", per_page: "number" },
};

/** Operations that upgrade to a WebSocket instead of returning a body. */
const STREAM_OPS = new Set(["get_v1_chatrooms_room_stream"]);

/**
 * Enum fields the contract declares open (`docs/API_CONTRACT.md` §3.2).
 * Their types keep the known literals for completion but accept any string.
 */
const OPEN_ENUM_FIELDS = new Set(["status", "kind", "event", "eventType", "provider"]);

const REF_PREFIX = "#/components/schemas/";

/** Turn a schema name or inline schema into a TypeScript type expression. */
function typeOf(schema) {
  if (!schema) return "unknown";
  if (schema.$ref) return refName(schema.$ref);
  if (Object.keys(schema).length === 0) return "unknown";

  if (Array.isArray(schema.enum)) {
    const literals = schema.enum.map((value) => JSON.stringify(value)).join(" | ");
    return literals;
  }

  const types = Array.isArray(schema.type) ? schema.type : [schema.type];
  const nullable = types.includes("null");
  const base = types.filter((type) => type !== "null");

  if (base.length > 1) {
    const union = base.map((type) => typeOf({ ...schema, type })).join(" | ");
    return nullable ? `${union} | null` : union;
  }

  const only = base[0];
  const rendered = (() => {
    switch (only) {
      case "string":
        return "string";
      case "integer":
      case "number":
        return "number";
      case "boolean":
        return "boolean";
      case "array": {
        const item = typeOf(schema.items);
        return needsParens(item) ? `(${item})[]` : `${item}[]`;
      }
      case "object": {
        if (schema.properties) {
          return `{ ${Object.entries(schema.properties)
            .map(([name, value]) => `${propertyName(name)}: ${typeOf(value)};`)
            .join(" ")} }`;
        }
        if (schema.additionalProperties) {
          const value =
            typeof schema.additionalProperties === "object"
              ? typeOf(schema.additionalProperties)
              : "unknown";
          return `Record<string, ${value}>`;
        }
        return "Record<string, unknown>";
      }
      case undefined:
        return "unknown";
      default:
        return "unknown";
    }
  })();
  return nullable ? `${rendered} | null` : rendered;
}

/** True when a type expression must be parenthesised before `[]`. */
function needsParens(type) {
  return type.includes(" | ") || type.startsWith("{ ");
}

/** Quote a property name only when it is not a plain identifier. */
function propertyName(name) {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
}

/** Schema name from a `$ref`. */
function refName(ref) {
  if (!ref.startsWith(REF_PREFIX)) {
    throw new Error(`unsupported $ref in contract: ${ref}`);
  }
  return ref.slice(REF_PREFIX.length);
}

/** Human-readable doc comment, or an empty string. */
function doc(description, indent = "") {
  if (!description) return "";
  const lines = String(description)
    .replace(/\*\//g, "* /")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) return "";
  return `${indent}/**\n${lines.map((line) => `${indent} * ${line}`).join("\n")}\n${indent} */\n`;
}

/** Render `src/generated/types.d.ts`. */
function renderTypes(spec) {
  const schemas = spec.components.schemas;
  const out = [];
  out.push("// Generated from docs/api/openapi-v1.json. Do not edit by hand.");
  out.push("// Regenerate: npm run generate (from sdk/ts).");
  out.push("");
  out.push("/** Any JSON value. */");
  out.push("export type JsonValue = string | number | boolean | null | JsonValue[] | JsonObject;");
  out.push("");
  out.push("/** A JSON object with opaque members. */");
  out.push("export interface JsonObject {");
  out.push("  [key: string]: JsonValue;");
  out.push("}");
  out.push("");
  out.push("/** A query-string value. */");
  out.push("export type QueryValue = string | number | boolean | undefined;");
  out.push("");
  out.push("/** Query-string values keyed by parameter name. */");
  out.push("export type QueryValues = Record<string, QueryValue>;");
  out.push("");
  const codes = schemas.Problem?.properties?.code?.enum ?? [];
  if (codes.length) {
    out.push("/** Stable hub problem codes (docs/HUB_API_ERRORS.md), plus unknown codes verbatim. */");
    out.push(`export type ProblemCode = ${codes.map((code) => JSON.stringify(code)).join(" | ")} | (string & {});`);
    out.push("");
  }

  const enumAliases = [];
  for (const [schemaName, schema] of Object.entries(schemas)) {
    if (schema.enum) {
      enumAliases.push(`export type ${schemaName} = ${typeOf(schema)};`);
      continue;
    }
    if (!schema.properties && !schema.type) {
      enumAliases.push(
        `${doc(schema.description)}export type ${schemaName} = JsonValue;`,
      );
      continue;
    }
    if (!schema.properties && schema.type === "object" && schema.additionalProperties) {
      enumAliases.push(
        `${doc(schema.description)}export type ${schemaName} = ${typeOf(schema)};`,
      );
      continue;
    }
    const required = new Set(schema.required ?? []);
    const lines = [];
    lines.push(`${doc(schema.description)}export interface ${schemaName} {`);
    for (const [propName, propSchema] of Object.entries(schema.properties ?? {})) {
      const optional = required.has(propName) ? "" : "?";
      let type = typeOf(propSchema);
      if (Array.isArray(propSchema.enum)) {
        if (OPEN_ENUM_FIELDS.has(propName)) {
          type = `${type} | (string & {})`;
        } else if (schemaName === "Problem" && propName === "code") {
          type = "ProblemCode";
        }
      }
      const comment = doc(propSchema.description, "  ");
      lines.push(`${comment}  ${propertyName(propName)}${optional}: ${type};`);
    }
    lines.push("}");
    out.push(lines.join("\n"));
    out.push("");
  }
  out.push(...enumAliases);
  out.push("");
  return out.join("\n");
}

/** Success statuses and their schema names for one operation. */
function successSchemas(operation) {
  const successes = new Map();
  for (const [status, response] of Object.entries(operation.responses ?? {})) {
    const code = Number(status);
    if (!Number.isFinite(code) || code < 200 || code >= 300) continue;
    const content = response.content ?? {};
    const json = content["application/json"] ?? content["*/*"];
    if (!json) {
      successes.set(code, { kind: "empty" });
      continue;
    }
    successes.set(code, { kind: "schema", schema: json.schema ?? {} });
  }
  return successes;
}

/** TypeScript type for one operation's success body. */
function responseTypeOf(operation) {
  const types = new Set();
  for (const value of successSchemas(operation).values()) {
    if (value.kind === "empty") continue;
    const schema = value.schema;
    if (schema.type === "object" && !schema.$ref && !schema.properties) {
      types.add("JsonObject");
    } else if (!schema.$ref && !schema.type && !schema.enum) {
      types.add("JsonValue");
    } else {
      types.add(typeOf(schema));
    }
  }
  if (!types.size) return "void";
  return [...types].join(" | ");
}

/** Request body type for one operation, or `null` when it takes no body. */
function bodyTypeOf(operation, method) {
  const content = operation.requestBody?.content ?? {};
  const json = content["application/json"] ?? content["*/*"];
  if (json) {
    const schema = json.schema ?? {};
    if (schema.type === "object" && !schema.$ref && !schema.properties) return "JsonObject";
    return typeOf(schema);
  }
  // The game surface documents no bodies; handlers still accept JSON.
  if (["POST", "PUT", "PATCH"].includes(method)) return "JsonObject";
  return null;
}

/** Query record for one operation. */
function queryTypeOf(operationId) {
  const query = QUERY[operationId];
  if (!query) return null;
  return query;
}

/** Build the derived route table. */
function deriveRoutes(spec) {
  const routes = [];
  for (const [path, operations] of Object.entries(spec.paths)) {
    for (const [method, operation] of Object.entries(operations)) {
      if (!["get", "post", "put", "patch", "delete"].includes(method)) continue;
      const operationId = operation.operationId;
      const naming = NAMES[operationId];
      if (!naming) {
        throw new Error(
          `contract route ${method.toUpperCase()} ${path} (${operationId}) has no SDK mapping; ` +
            "add it to NAMES in scripts/lib/contract.mjs",
        );
      }
      const pathParams = (operation.parameters ?? [])
        .filter((param) => param.in === "path")
        .map((param) => param.name);
      const errorStatuses = Object.keys(operation.responses ?? {})
        .map(Number)
        .filter((code) => code >= 400 && code < 500 && code !== 429)
        .sort((a, b) => a - b);
      const declaresRateLimit = Object.keys(operation.responses ?? {}).includes("429");
      routes.push({
        operationId,
        method: method.toUpperCase(),
        path,
        namespace: naming.slice(0, -1),
        name: naming[naming.length - 1],
        pathParams,
        query: queryTypeOf(operationId),
        body: bodyTypeOf(operation, method.toUpperCase()),
        response: STREAM_OPS.has(operationId) ? "ChatStream" : responseTypeOf(operation),
        success: [...successSchemas(operation).keys()].sort((a, b) => a - b),
        errors: errorStatuses,
        rateLimited: declaresRateLimit,
        auth: operation["x-auth"] ?? "public",
        scope: operation["x-required-scope"] ?? null,
        role: operation["x-required-role"] ?? null,
        surface: operation["x-surface"] ?? "unknown",
        deprecated: operation.deprecated === true,
        summary: operation.summary ?? "",
      });
    }
  }
  routes.sort((a, b) => (a.operationId < b.operationId ? -1 : 1));
  return routes;
}

/** Render `src/generated/routes.js`. */
function renderRoutes(spec, routes) {
  const table = routes.map((route) => {
    const fields = [
      `operationId: ${JSON.stringify(route.operationId)}`,
      `method: ${JSON.stringify(route.method)}`,
      `path: ${JSON.stringify(route.path)}`,
      `namespace: ${JSON.stringify(route.namespace)}`,
      `name: ${JSON.stringify(route.name)}`,
      `pathParams: ${JSON.stringify(route.pathParams)}`,
      `query: ${JSON.stringify(route.query)}`,
      `body: ${JSON.stringify(route.body)}`,
      `response: ${JSON.stringify(route.response)}`,
      `success: ${JSON.stringify(route.success)}`,
      `errors: ${JSON.stringify(route.errors)}`,
      `rateLimited: ${JSON.stringify(route.rateLimited)}`,
      `auth: ${JSON.stringify(route.auth)}`,
      `scope: ${JSON.stringify(route.scope)}`,
      `role: ${JSON.stringify(route.role)}`,
      `surface: ${JSON.stringify(route.surface)}`,
      `deprecated: ${JSON.stringify(route.deprecated)}`,
      `summary: ${JSON.stringify(route.summary)}`,
    ];
    return `  ${JSON.stringify(route.operationId)}: {\n${fields
      .map((field) => `    ${field},`)
      .join("\n")}\n  },`;
  });
  return [
    "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
    "// Regenerate: npm run generate (from sdk/ts).",
    "",
    `export const CONTRACT_VERSION = ${JSON.stringify(spec.info.version)};`,
    "",
    "/** Every route in the committed contract, keyed by operation id. */",
    "export const ROUTES = Object.freeze({",
    ...table,
    "});",
    "",
    "/** Operation ids in contract order. */",
    "export const OPERATIONS = Object.freeze([",
    ...routes.map((route) => `  ${JSON.stringify(route.operationId)},`),
    "]);",
    "",
    "/** Scopes a key may hold (docs/API_CONTRACT.md §4). */",
    'export const SCOPES = Object.freeze(["projects:read", "configuration:validate", "configuration:install", "runs:dispatch", "daemons:enroll"]);',
    "",
    "/** Roles a member may hold (docs/API_CONTRACT.md §4). */",
    'export const ROLES = Object.freeze(["owner", "admin", "member", "viewer"]);',
    "",
    "/** Route metadata by operation id, or undefined. */",
    "export function route(operationId) {",
    "  return ROUTES[operationId];",
    "}",
    "",
  ].join("\n");
}

/** Default parameter name when a path parameter is not a valid identifier. */
function paramName(name) {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : `param${name.replace(/\W/g, "")}`;
}

/** Argument list for a route: path params, body, options. */
function signatureOf(route) {
  const args = [];
  if (route.pathParams.length === 1) {
    args.push({ name: paramName(route.pathParams[0]), type: "string", required: true });
  } else if (route.pathParams.length > 1) {
    const members = route.pathParams
      .map((name) => `${propertyName(paramName(name))}: string;`)
      .join(" ");
    args.push({ name: "params", type: `{ ${members} }`, required: true });
  }
  if (route.body) {
    args.push({ name: "body", type: route.body, required: true });
  }
  const queryType = route.query
    ? `RequestOptions<{ ${Object.entries(route.query)
        .map(([name, type]) => `${propertyName(name)}?: ${type};`)
        .join(" ")} }>`
    : "RequestOptions";
  args.push({ name: "options", type: queryType, required: false });
  return args;
}

/** TypeScript parameter list. */
function paramList(signature) {
  return signature
    .map((arg) => (arg.required ? `${arg.name}: ${arg.type}` : `${arg.name}?: ${arg.type}`))
    .join(", ");
}

/** JavaScript argument list. */
function jsParamList(signature) {
  return signature.map((arg) => arg.name).join(", ");
}

/** Render `src/generated/resources.d.ts`. */
function renderResourceTypes(spec, routes) {
  /** @type {Map<string, {methods: Map<string, string>, children: Map<string, any>}>} */
  const root = { methods: new Map(), children: new Map() };
  for (const route of routes) {
    let node = root;
    for (const segment of route.namespace) {
      if (!node.children.has(segment)) {
        node.children.set(segment, { methods: new Map(), children: new Map() });
      }
      node = node.children.get(segment);
    }
    const signature = signatureOf(route);
    const lines = [];
    if (route.summary) {
      lines.push(...doc(route.summary, "    ").trimEnd().split("\n"));
    }
    const returns = route.response === "ChatStream" ? "ChatStream" : `Promise<${route.response}>`;
    lines.push(`    ${route.name}(${paramList(signature)}): ${returns};`);
    node.methods.set(route.name, lines);
  }

  function renderNode(node, indent) {
    const lines = [];
    for (const [segment, child] of [...node.children.entries()].sort()) {
      lines.push(`${indent}${segment}: {`);
      lines.push(...renderNode(child, `${indent}  `));
      lines.push(`${indent}};`);
    }
    for (const method of [...node.methods.entries()].sort(([a], [b]) => (a < b ? -1 : 1))) {
      lines.push(...method[1].map((line) => line.replace(/^    /, indent)));
    }
    return lines;
  }

  const body = renderNode(root, "  ");
  const rendered = body.join("\n");
  const known = new Set(Object.keys(spec.components.schemas));
  const used = [...known].filter((name) => new RegExp(`\\b${name}\\b`).test(rendered));
  if (rendered.includes("JsonObject")) used.push("JsonObject");
  return [
    "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
    "// Regenerate: npm run generate (from sdk/ts).",
    "",
    'import type { ChatStream, RequestOptions } from "../stream.js";',
    'import type {',
    ...[...new Set(used)].sort().map((name) => `  ${name},`),
    '} from "./types.js";',
    "",
    "/** One typed method per resource and command in the contract. */",
    "export interface Resources {",
    ...body,
    "}",
    "",
    "/** Bind every typed method to one client instance. */",
    'export declare function buildResources(client: import("../client.js").TidepoolClient): Resources;',
    "",
  ].join("\n");
}

/** Render `src/generated/resources.js`. */
function renderResourcesJs(routes) {
  /** @type {Map<string, any>} */
  const root = new Map();
  for (const route of routes) {
    let node = { methods: [], children: root };
    for (const segment of route.namespace) {
      if (!node.children.has(segment)) {
        node.children.set(segment, { methods: [], children: new Map() });
      }
      node = node.children.get(segment);
    }
    node.methods.push(route);
  }

  function renderNode(node, indent) {
    const lines = [];
    for (const [segment, child] of [...node.children.entries()].sort()) {
      lines.push(`${indent}${segment}: {`);
      lines.push(...renderNode(child, `${indent}  `));
      lines.push(`${indent}},`);
    }
    for (const route of [...node.methods].sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const signature = signatureOf(route);
      const delegate = route.response === "ChatStream" ? "_stream" : "_call";
      const args = [];
      args.push(JSON.stringify(route.operationId));
      args.push("options");
      if (route.pathParams.length) {
        const source = route.pathParams.length === 1 ? paramName(route.pathParams[0]) : "params";
        args.push(source);
      } else {
        args.push("undefined");
      }
      args.push(route.body ? "body" : "undefined");
      lines.push(
        `${indent}${route.name}(${jsParamList(signature)}) {`,
        `${indent}  return client.${delegate}(${args.join(", ")});`,
        `${indent}},`,
      );
    }
    return lines;
  }

  return [
    "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
    "// Regenerate: npm run generate (from sdk/ts).",
    "",
    "/**",
    " * Bind every typed method to one client instance.",
    " *",
    " * @param {import('../client.js').TidepoolClient} client",
    " */",
    "export function buildResources(client) {",
    "  return {",
    ...renderNode({ methods: [], children: root }, "    "),
    "  };",
    "}",
    "",
  ].join("\n");
}

/** Render the type-conformance file. */
function renderConformance(spec, routes) {
  const known = Object.keys(spec.components.schemas);
  const referenced = routes.map((route) => `${route.response} ${route.body ?? ""}`).join(" ");
  const responseTypes = new Set(
    known.filter((name) => new RegExp(`\\b${name}\\b`).test(referenced)),
  );
  if (referenced.includes("JsonObject")) responseTypes.add("JsonObject");
  const lines = [
    "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
    "// Typecheck only: npm run typecheck. Not executed.",
    "",
    ...(referenced.includes("ChatStream") ? ['import type { ChatStream } from "../../src/stream.js";'] : []),
    'import type { TidepoolClient } from "../../src/index.js";',
    "import type {",
    ...[...responseTypes].sort().map((name) => `  ${name},`),
    '} from "../../src/generated/types.js";',
    "",
    "declare const client: TidepoolClient;",
    "",
  ];
  for (const route of routes) {
    const signature = signatureOf(route);
    const args = signature.map((arg) => {
      if (arg.name === "options") return "";
      if (arg.name === "body") return `{} as ${arg.type}`;
      if (arg.type.startsWith("{ ")) return `{} as ${arg.type}`;
      return JSON.stringify("id-1");
    });
    const call = `client.${[...route.namespace, route.name].join(".")}(${args.join(", ")})`;
    if (route.response === "ChatStream") {
      lines.push(`const ${route.operationId}: ChatStream = ${call};`);
    } else {
      lines.push(`const ${route.operationId}: Promise<${route.response}> = ${call};`);
    }
    lines.push(`void ${route.operationId};`);
    if (route.body && !["RawJson", "JsonValue", "unknown"].includes(route.body)) {
      lines.push("// @ts-expect-error the request body is typed by the contract");
      lines.push(
        `void client.${[...route.namespace, route.name].join(".")}(${signature
          .map((arg) => (arg.name === "body" ? "42" : arg.name === "options" ? "" : JSON.stringify("id-1")))
          .join(", ")})`,
      );
    }
    lines.push("");
  }
  return lines.join("\n");
}

/** Render `src/generated/routes.d.ts`. */
function renderRoutesDts() {
  return [
    "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
    "// Regenerate: npm run generate (from sdk/ts).",
    "",
    "/** One route's access model and wire shape, from the contract. */",
    "export interface RouteInfo {",
    "  readonly operationId: string;",
    "  readonly method: string;",
    "  readonly path: string;",
    "  readonly namespace: readonly string[];",
    "  readonly name: string;",
    "  readonly pathParams: readonly string[];",
    "  readonly query: Readonly<Record<string, string>> | null;",
    "  readonly body: string | null;",
    "  readonly response: string;",
    "  readonly success: readonly number[];",
    "  readonly errors: readonly number[];",
    "  readonly rateLimited: boolean;",
    "  readonly auth: \"public\" | \"member\" | \"member_or_api_key\" | \"api_key\" | \"host\" | \"signature\";",
    "  readonly scope: string | null;",
    "  readonly role: string | null;",
    "  readonly surface: string;",
    "  readonly deprecated: boolean;",
    "  readonly summary: string;",
    "}",
    "",
    "/** Contract version of the committed document. */",
    "export declare const CONTRACT_VERSION: string;",
    "",
    "/** Every route in the committed contract, keyed by operation id. */",
    "export declare const ROUTES: Readonly<Record<string, RouteInfo>>;",
    "",
    "/** Operation ids in contract order. */",
    "export declare const OPERATIONS: readonly string[];",
    "",
    "/** API-key scopes. */",
    "export declare const SCOPES: readonly string[];",
    "",
    "/** Member roles. */",
    "export declare const ROLES: readonly string[];",
    "",
    "/** Route metadata by operation id, or undefined. */",
    "export declare function route(operationId: string): RouteInfo | undefined;",
    "",
  ].join("\n");
}

/** Render every artifact, keyed by path relative to `sdk/ts`. */
export function render(spec) {
  const routes = deriveRoutes(spec);
  return {
    "src/generated/types.d.ts": renderTypes(spec),
    "src/generated/routes.js": renderRoutes(spec, routes),
    "src/generated/routes.d.ts": renderRoutesDts(),
    "src/generated/resources.js": renderResourcesJs(routes),
    "src/generated/resources.d.ts": renderResourceTypes(spec, routes),
    "test/generated/conformance.ts": renderConformance(spec, routes),
    routes,
  };
}
