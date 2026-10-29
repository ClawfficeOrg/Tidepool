#!/usr/bin/env node
//! Contract reader and emitter for the Rust SDK.
//!
//! `docs/api/openapi-v1.json` is the only input. This script renders four
//! committed artifacts, deterministically, so `--check` can regenerate them in
//! memory and compare byte for byte:
//!
//! - `src/generated/types.rs`    — one serde struct per contract schema;
//! - `src/generated/routes.rs`   — the typed route table the clients dispatch on;
//! - `src/generated/endpoints.rs`— one typed request type per contract route;
//! - `src/generated/resources.rs`— namespace wrappers for the blocking and
//!   async clients.
//!
//! Every route in the document must have an explicit `{group, name}` mapping
//! below. A route without a mapping fails generation rather than shipping
//! untyped, which is the same rule the TypeScript generator applies.
//!
//! The generated modules are checked into the repository and the crate's tests
//! hash the committed contract, so a contract edit without a regeneration
//! fails `cargo test` even where Node is unavailable.

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const HERE = dirname(fileURLToPath(import.meta.url));
const CRATE = resolve(HERE, "..");
const CONTRACT = resolve(CRATE, "../../docs/api/openapi-v1.json");
const REF_PREFIX = "#/components/schemas/";

/** Operation id to `[namespace path..., method name]`, mirroring sdk/ts. */
const NAMES = {
  get_healthz: ["system", "health"],
  get_metrics: ["system", "metrics"],
  get_openapi_json: ["system", "openapi"],
  get_openapi_v1_json: ["system", "openapiV1"],

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
  post_v1_pool_logout: ["pool", "logout"],

  get_v1_api_keys: ["apiKeys", "list"],
  post_v1_api_keys: ["apiKeys", "create"],
  get_v1_api_keys_whoami: ["apiKeys", "whoami"],
  delete_v1_api_keys_id: ["apiKeys", "remove"],

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
  post_v1_hosts_id_executions_execution_id_interrupt: ["hosts", "interruptExecution"],

  get_v1_connections: ["connections", "list"],
  post_v1_connections: ["connections", "create"],
  get_v1_connections_id: ["connections", "get"],
  patch_v1_connections_id: ["connections", "update"],
  delete_v1_connections_id: ["connections", "remove"],

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

  post_v1_triggers_validate: ["triggers", "validate"],
  post_v1_triggers_install: ["triggers", "install"],
  post_v1_runs: ["runs", "dispatch"],

  get_v1_activity: ["activity", "list"],
  get_v1_activity_runs_run: ["activity", "run"],

  get_v1_chatrooms: ["chatrooms", "list"],
  get_v1_chatrooms_room_members: ["chatrooms", "members"],
  get_v1_chatrooms_room_messages: ["chatrooms", "messages"],
  post_v1_chatrooms_room_messages: ["chatrooms", "send"],
  get_v1_chatrooms_room_stream: ["chatrooms", "stream"],

  get_v1_sessions_session_relay: ["relay", "get"],
  put_v1_sessions_session_relay: ["relay", "enable"],
  delete_v1_sessions_session_relay: ["relay", "stop"],
  get_v1_sessions_session_relay_events: ["relay", "events"],
  post_v1_sessions_session_relay_events: ["relay", "publish"],

  post_v1_webhooks_github_slug: ["webhooks", "github"],
  post_v1_webhooks_slack_slug: ["webhooks", "slack"],

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
 * Query parameters read by handlers but absent from the document
 * (`docs/API_CONTRACT.md` §5.1). Mirrors the TypeScript SDK's table.
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

/** One operation upgrades to a WebSocket instead of returning a JSON body. */
const STREAM_OPS = new Set(["get_v1_chatrooms_room_stream"]);

const KEYWORDS = new Set(
  "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use virtual where while yield try union".split(
    " ",
  ),
);

const HEADER = [
  "// Generated from docs/api/openapi-v1.json. Do not edit by hand.",
  "// Regenerate: node scripts/generate.mjs (from sdk/rust).",
  "",
];

/** snake_case a camelCase or kebab-case identifier. */
function snake(name) {
  return name
    .replace(/[-.\s]+/g, "_")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
    .toLowerCase();
}

/** PascalCase an operation id or schema name. */
function pascal(name) {
  return snake(name)
    .split("_")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

/** A Rust field name for a JSON property name. */
function field(name) {
  const base = snake(name);
  if (KEYWORDS.has(base)) return `${base}_`;
  return base;
}

/** JSON property names whose Rust field name changes need a serde rename. */
function renameAttr(jsonName, rustName) {
  return jsonName === rustName ? "" : `    #[serde(rename = ${JSON.stringify(jsonName)})]\n`;
}

/** Doc comment for a description, or the empty string. */
function doc(description, indent = "") {
  if (!description) return "";
  const lines = String(description)
    .replace(/\*\//g, "* /")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) return "";
  return `${lines.map((line) => `${indent}/// ${line}`).join("\n")}\n`;
}

/** True when a schema allows null. */
function nullable(schema) {
  return Array.isArray(schema?.type) && schema.type.includes("null");
}

/** Known values of an enum field, for the doc comment. */
function enumNote(schema) {
  if (!Array.isArray(schema?.enum)) return "";
  const values = schema.enum.map((value) => JSON.stringify(value)).join(", ");
  return `Known values: ${values}. Unknown values are tolerated (contract §3.2).`;
}

/** Base Rust type for a schema, ignoring nullability. */
function baseType(schema, inTypesModule) {
  if (!schema || Object.keys(schema).length === 0) return "JsonValue";
  if (schema.$ref) {
    if (!schema.$ref.startsWith(REF_PREFIX)) {
      throw new Error(`unsupported $ref: ${schema.$ref}`);
    }
    const name = schema.$ref.slice(REF_PREFIX.length);
    return inTypesModule ? name : `crate::types::${name}`;
  }
  const types = Array.isArray(schema.type) ? schema.type : [schema.type];
  const only = types.find((type) => type !== "null");
  switch (only) {
    case "string":
      return "String";
    case "integer":
      return "i64";
    case "number":
      return "f64";
    case "boolean":
      return "bool";
    case "array":
      return `Vec<${fullType(schema.items, inTypesModule)}>`;
    case "object":
      if (schema.additionalProperties) {
        const value =
          typeof schema.additionalProperties === "object"
            ? fullType(schema.additionalProperties, inTypesModule)
            : "JsonValue";
        return `std::collections::HashMap<String, ${value}>`;
      }
      return "JsonObject";
    default:
      return "JsonValue";
  }
}

/** Rust type for a schema, wrapping in `Option` when it may be null. */
function fullType(schema, inTypesModule = false) {
  const base = baseType(schema, inTypesModule);
  if (nullable(schema) && !base.startsWith("Option<")) return `Option<${base}>`;
  return base;
}

/** Field type: optional fields are `Option<T>` even when the schema is not. */
function fieldType(propSchema, required) {
  const base = fullType(propSchema, true);
  if (!required && !base.startsWith("Option<")) return `Option<${base}>`;
  return base;
}

/** Render `src/generated/types.rs`. */
function renderTypes(spec) {
  const schemas = spec.components.schemas;
  const out = [...HEADER];
  out.push("//! Generated contract types: one struct per schema in the document.");
  out.push("");
  out.push("#![allow(clippy::struct_field_names)]");
  out.push("");
  out.push("use serde::{Deserialize, Serialize};");
  out.push("");
  out.push("/// Any JSON value.");
  out.push("pub type JsonValue = serde_json::Value;");
  out.push("");
  out.push("/// A JSON object with members this SDK does not name.");
  out.push("pub type JsonObject = serde_json::Map<String, JsonValue>;");
  out.push("");
  out.push("/// A JSON array whose members this SDK does not name.");
  out.push("pub type JsonArray = Vec<JsonValue>;");
  out.push("");
  out.push("/// The stable problem codes the contract documents, plus any future code.");
  out.push("pub mod problem_code {");
  for (const code of schemas.Problem?.properties?.code?.enum ?? []) {
    out.push(`    /// \`${code}\``);
    out.push(`    pub const ${code.toUpperCase()}: &str = ${JSON.stringify(code)};`);
  }
  out.push("}");
  out.push("");

  for (const [name, schema] of Object.entries(schemas)) {
    if (name === "RawJson") {
      out.push(...doc(schema.description).trimEnd().split("\n").filter(Boolean));
      out.push("pub type RawJson = JsonValue;");
      out.push("");
      continue;
    }
    const description = doc(schema.description);
    if (Array.isArray(schema.enum)) {
      // The document has no top-level enums today; keep the renderer honest.
      out.push(`${description}pub type ${name} = String;`);
      out.push("");
      continue;
    }
    if (!schema.properties && schema.type === "array") {
      out.push(`${description}pub type ${name} = ${baseType(schema, true)};`);
      out.push("");
      continue;
    }
    if (!schema.properties && !schema.type) {
      out.push(`${description}pub type ${name} = JsonValue;`);
      out.push("");
      continue;
    }
    if (!schema.properties) {
      out.push(`${description}pub type ${name} = ${baseType(schema, true)};`);
      out.push("");
      continue;
    }
    const required = new Set(schema.required ?? []);
    const lines = [];
    lines.push(...description.trimEnd().split("\n").filter(Boolean));
    lines.push("#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]");
    lines.push(`pub struct ${name} {`);
    for (const [jsonName, propSchema] of Object.entries(schema.properties)) {
      const rustName = name === "Problem" && jsonName === "type" ? "problem_type" : field(jsonName);
      // `Problem` is parsed from whatever a failing server sent, so only `code`
      // is treated as required and the remaining fields degrade to `None`.
      const isRequired = name === "Problem" ? jsonName === "code" : required.has(jsonName);
      const notes = [propSchema.description, enumNote(propSchema)].filter(Boolean).join(" ");
      const comment = doc(notes || `\`${jsonName}\``, "    ");
      if (comment) lines.push(...comment.trimEnd().split("\n"));
      lines.push(...renameAttr(jsonName, rustName).trimEnd().split("\n").filter(Boolean));
      const type = name === "Problem" && jsonName !== "code" ? `Option<${baseType(propSchema, true)}>` : fieldType(propSchema, isRequired);
      lines.push(`    pub ${rustName}: ${type},`);
    }
    lines.push("}");
    out.push(lines.join("\n"));
    out.push("");
  }
  return `${out.join("\n").trimEnd()}\n`;
}

/** Success statuses and their response schemas for one operation. */
function successes(operation) {
  const map = new Map();
  for (const [status, response] of Object.entries(operation.responses ?? {})) {
    const code = Number(status);
    if (!Number.isFinite(code) || code < 200 || code >= 300) continue;
    const content = response.content ?? {};
    const json = content["application/json"] ?? content["*/*"];
    if (!json) {
      map.set(code, { kind: "empty" });
      continue;
    }
    map.set(code, { kind: "schema", schema: json.schema ?? {} });
  }
  return map;
}

/** Rust type for an operation's success body. */
function responseType(operation, inTypesModule = false) {
  const seen = new Map();
  for (const value of successes(operation).values()) {
    if (value.kind === "empty") continue;
    const schema = value.schema;
    let type;
    if (schema.type === "object" && !schema.$ref && !schema.properties) {
      type = "JsonObject";
    } else if (!schema.$ref && !schema.type && !schema.enum) {
      type = "JsonValue";
    } else {
      type = baseType(schema, inTypesModule);
    }
    seen.set(type, schema);
  }
  if (!seen.size) return { type: "()", multiple: false, schemas: [] };
  if (seen.size === 1) {
    const [[type]] = seen;
    return { type, multiple: false, schemas: [] };
  }
  return { type: null, multiple: true, schemas: [...seen.values()] };
}

/** Name of the generated response enum for a route with several success bodies. */
function unionName(operationId) {
  return `${pascal(operationId)}Response`;
}

/** Rust type for an operation's request body. */
function bodyType(operation, method) {
  const json = (operation.requestBody?.content ?? {})["application/json"];
  if (json) {
    const schema = json.schema ?? {};
    if (schema.type === "object" && !schema.$ref && !schema.properties) return "crate::types::JsonObject";
    return qualifyBody(baseType(schema, false));
  }
  // The game surface documents no bodies; its handlers still accept JSON.
  if (["POST", "PUT", "PATCH"].includes(method)) return "crate::types::JsonValue";
  return null;
}

/** Fully qualify a JSON alias used as a request body. */
function qualifyBody(type) {
  return type === "JsonValue" || type === "JsonObject" ? `crate::types::${type}` : type;
}

/** Query parameter record for one operation. */
function queryOf(operationId) {
  return QUERY[operationId] ?? null;
}

/** Build the route table, failing on any unmapped operation. */
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
            "add it to NAMES in sdk/rust/scripts/generate.mjs",
        );
      }
      const pathParams = (operation.parameters ?? [])
        .filter((param) => param.in === "path")
        .map((param) => param.name);
      const response = STREAM_OPS.has(operationId)
        ? { type: "ChatStream", multiple: false, schemas: [] }
        : operationId === "get_metrics"
          ? { type: "String", multiple: false, schemas: [] }
          : responseType(operation, true);
      const errorStatuses = Object.keys(operation.responses ?? {})
        .map(Number)
        .filter((code) => code >= 400 && code < 500 && code !== 429)
        .sort((a, b) => a - b);
      routes.push({
        operationId,
        method: method.toUpperCase(),
        path,
        namespace: naming.slice(0, -1),
        name: naming[naming.length - 1],
        pathParams,
        query: queryOf(operationId),
        body: bodyType(operation, method.toUpperCase()),
        responseType: response.multiple ? unionName(operationId) : response.type,
        responseSchemas: response.schemas,
        success: [...successes(operation).keys()].sort((a, b) => a - b),
        errors: errorStatuses,
        rateLimited: Object.keys(operation.responses ?? {}).includes("429"),
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

const AUTH = {
  public: "Auth::Public",
  member: "Auth::Member",
  member_or_api_key: "Auth::MemberOrApiKey",
  api_key: "Auth::ApiKey",
  host: "Auth::Host",
  signature: "Auth::Signature",
};

const SURFACE = {
  system: "Surface::System",
  hub: "Surface::Hub",
  game: "Surface::Game",
  unknown: "Surface::Unknown",
};

/** Render `src/generated/routes.rs`. */
function renderRoutes(spec, routes, hash) {
  const out = [...HEADER];
  out.push("//! Generated route table: the contract's access model and wire shape.");
  out.push("");
  out.push("pub use crate::route::{Auth, Method, RouteInfo, Surface};");
  out.push("");
  out.push(`/// SHA-256 of the contract this module was generated from.`);
  out.push(`pub const CONTRACT_SHA256: &str = ${JSON.stringify(hash)};`);
  out.push("");
  out.push("/// Contract version reported by the committed document.");
  out.push(`pub const CONTRACT_VERSION: &str = ${JSON.stringify(spec.info.version)};`);
  out.push("");
  out.push("/// API-key scopes a key may hold (`docs/API_CONTRACT.md` §4).");
  out.push(
    `pub const SCOPES: &[&str] = &["projects:read", "configuration:validate", "configuration:install", "runs:dispatch", "daemons:enroll"];`,
  );
  out.push("");
  out.push("/// Member roles (`docs/API_CONTRACT.md` §4).");
  out.push(`pub const ROLES: &[&str] = &["owner", "admin", "member", "viewer"];`);
  out.push("");
  for (const route of routes) {
    out.push(`/// \`${route.method} ${route.path}\` — ${route.summary}`);
    out.push(`pub static ROUTE_${route.operationId.toUpperCase()}: RouteInfo = RouteInfo {`);
    out.push(`    operation_id: ${JSON.stringify(route.operationId)},`);
    out.push(`    method: Method::${pascal(route.method)},`);
    out.push(`    path: ${JSON.stringify(route.path)},`);
    out.push(`    path_params: &[${route.pathParams.map((name) => JSON.stringify(name)).join(", ")}],`);
    out.push(`    query_params: &[${Object.keys(route.query ?? {}).map((name) => JSON.stringify(name)).join(", ")}],`);
    out.push(`    has_body: ${route.body !== null},`);
    out.push(`    response: ${JSON.stringify(route.responseType)},`);
    out.push(`    success: &[${route.success.join(", ")}],`);
    out.push(`    errors: &[${route.errors.join(", ")}],`);
    out.push(`    rate_limited: ${route.rateLimited},`);
    out.push(`    auth: ${AUTH[route.auth] ?? "Auth::Unknown"},`);
    out.push(`    scope: ${route.scope ? `Some(${JSON.stringify(route.scope)})` : "None"},`);
    out.push(`    role: ${route.role ? `Some(${JSON.stringify(route.role)})` : "None"},`);
    out.push(`    surface: ${SURFACE[route.surface] ?? "Surface::Unknown"},`);
    out.push(`    deprecated: ${route.deprecated},`);
    out.push(`    summary: ${JSON.stringify(route.summary)},`);
    out.push("};");
    out.push("");
  }
  out.push("/// Every route in the committed contract, in operation-id order.");
  out.push("pub static ROUTES: &[&RouteInfo] = &[");
  for (const route of routes) {
    out.push(`    &ROUTE_${route.operationId.toUpperCase()},`);
  }
  out.push("];");
  out.push("");
  out.push("/// Route metadata by operation id, or `None` when the id is not in the contract.");
  out.push("pub fn route(operation_id: &str) -> Option<&'static RouteInfo> {");
  out.push("    ROUTES");
  out.push("        .iter()");
  out.push("        .copied()");
  out.push("        .find(|route| route.operation_id == operation_id)");
  out.push("}");
  out.push("");
  return out.join("\n");
}

/** Rust type name for a route's query record. */
function queryTypeName(route) {
  return `${pascal(route.operationId)}Query`;
}

/** Rust type name for a route's typed request. */
function endpointName(route) {
  return pascal(route.operationId);
}

/** Constructor arguments for one route's request type. */
function argsOf(route) {
  const args = route.pathParams.map((name) => ({
    name: field(name),
    json: name,
    type: "impl Into<String>",
  }));
  if (route.body) args.push({ name: "body", json: null, type: route.body });
  return args;
}

/** Render `src/generated/endpoints.rs`. */
function renderEndpoints(routes) {
  const out = [...HEADER];
  out.push("//! Generated request types: one per contract route.");
  out.push("//!");
  out.push("//! Each type implements [`crate::route::Endpoint`], which binds it to the");
  out.push("//! route table and gives it a typed response body and request body.");
  out.push("");
  out.push("#![allow(clippy::redundant_field_names)]");
  out.push("#![allow(clippy::needless_update)]");
  out.push("");
  out.push("use crate::route::Endpoint;");
  out.push("");

  for (const route of routes) {
    if (route.responseSchemas.length) {
      out.push(`/// Success body of \`${route.operationId}\`, whose contract declares more than one.`);
      out.push("#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]");
      out.push("#[serde(untagged)]");
      out.push(`pub enum ${unionName(route.operationId)} {`);
      for (const schema of route.responseSchemas) {
        const name = schema.$ref.slice(REF_PREFIX.length);
        out.push(`    /// A \`${name}\` body.`);
        out.push(`    ${name}(${`crate::types::${name}`}),`);
      }
      out.push("}");
      out.push("");
    }
  }

  for (const route of routes) {
    if (route.query) {
      out.push(`/// Query parameters for \`${route.operationId}\`.`);
      out.push("///");
      out.push("/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);");
      out.push("/// the names mirror the handler structs and the TypeScript SDK.");
      out.push("#[derive(Debug, Clone, Default, PartialEq)]");
      out.push(`pub struct ${queryTypeName(route)} {`);
      for (const [name, kind] of Object.entries(route.query)) {
        out.push(`    /// \`${name}\``);
        out.push(`    pub ${field(name)}: Option<${kind === "number" ? "u64" : "String"}>,`);
      }
      out.push("}");
      out.push("");
      out.push(`impl ${queryTypeName(route)} {`);
      out.push("    fn to_pairs(&self) -> Vec<(&'static str, String)> {");
      out.push("        let mut pairs = Vec::new();");
      for (const [name, kind] of Object.entries(route.query)) {
        const rustName = field(name);
        if (kind === "number") {
          out.push(`        if let Some(value) = self.${rustName} {`);
          out.push(`            pairs.push((${JSON.stringify(name)}, value.to_string()));`);
          out.push("        }");
        } else {
          out.push(`        if let Some(value) = &self.${rustName} {`);
          out.push(`            pairs.push((${JSON.stringify(name)}, value.clone()));`);
          out.push("        }");
        }
      }
      out.push("        pairs");
      out.push("    }");
      out.push("}");
      out.push("");
    }

    const args = argsOf(route);
    const deprecation = route.deprecated
      ? `#[deprecated(note = "the contract marks ${route.operationId} deprecated")]\n`
      : "";
    out.push(...doc(route.summary).trimEnd().split("\n").filter(Boolean));
    out.push(`/// \`${route.method} ${route.path}\`.`);
    if (deprecation) out.push(deprecation.trimEnd());
    out.push("#[derive(Debug, Clone, Default, PartialEq)]");
    out.push(`pub struct ${endpointName(route)} {`);
    for (const name of route.pathParams) {
      out.push(`    /// Path parameter \`${name}\`.`);
      out.push(`    pub ${field(name)}: String,`);
    }
    if (route.query) {
      out.push("    /// Query parameters.");
      out.push(`    pub query: ${queryTypeName(route)},`);
    }
    if (route.body) {
      out.push("    /// Request body.");
      out.push(`    pub body: ${route.body},`);
    }
    out.push("}");
    out.push("");
    out.push(`impl ${endpointName(route)} {`);
    const params = args.map((arg) => `${arg.name}: ${arg.type}`).join(", ");
    if (args.length) {
      out.push(`    /// Build the request.`);
      out.push(`    pub fn new(${params}) -> Self {`);
      out.push("        Self {");
      for (const arg of args) {
        out.push(`            ${arg.name}: ${arg.json ? `${arg.name}.into()` : arg.name},`);
      }
      out.push("            ..Self::default()");
      out.push("        }");
      out.push("    }");
    } else {
      out.push("    /// Build the request.");
      out.push("    pub fn new() -> Self {");
      out.push("        Self::default()");
      out.push("    }");
    }
    if (route.query) {
      out.push("");
      out.push("    /// Replace the query parameters.");
      out.push(`    pub fn with_query(mut self, query: ${queryTypeName(route)}) -> Self {`);
      out.push("        self.query = query;");
      out.push("        self");
      out.push("    }");
    }
    out.push("}");
    out.push("");
    out.push(`impl Endpoint for ${endpointName(route)} {`);
    let responseRust;
    if (route.responseType === "ChatStream") {
      responseRust = "crate::stream::ChatStream";
    } else if (route.responseType === "()" || route.responseType === "String") {
      responseRust = route.responseType;
    } else if (route.responseSchemas.length) {
      responseRust = route.responseType;
    } else {
      responseRust = `crate::types::${route.responseType}`;
    }
    out.push(`    type Response = ${responseRust};`);
    out.push(`    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_${route.operationId.toUpperCase()};`);
    if (route.pathParams.length) {
      const pairs = route.pathParams
        .map((name) => `(${JSON.stringify(name)}, self.${field(name)}.as_str())`)
        .join(", ");
      out.push("    fn path(&self) -> String {");
      out.push(`        crate::route::path_with(Self::ROUTE.path, &[${pairs}])`);
      out.push("    }");
    } else {
      out.push("    fn path(&self) -> String {");
      out.push("        Self::ROUTE.path.to_owned()");
      out.push("    }");
    }
    if (route.query) {
      out.push("    fn query(&self) -> Vec<(&'static str, String)> {");
      out.push("        self.query.to_pairs()");
      out.push("    }");
    }
    if (route.body) {
      out.push("    fn body(&self) -> crate::Result<Option<Vec<u8>>> {");
      out.push("        Ok(Some(serde_json::to_vec(&self.body)?))");
      out.push("    }");
    }
    if (route.operationId === "get_metrics") {
      out.push("    fn decode_response(body: &[u8]) -> crate::Result<Self::Response> {");
      out.push("        Ok(String::from_utf8_lossy(body).into_owned())");
      out.push("    }");
    }
    out.push("}");
    out.push("");
  }

  out.push("#[cfg(test)]");
  out.push("mod typed {");
  out.push("    use super::*;");
  out.push("");
  out.push("    fn assert_endpoint<E: Endpoint>() {}");
  out.push("");
  out.push("    #[test]");
  out.push("    fn every_route_has_a_typed_endpoint() {");
  for (const route of routes) {
    out.push(`        assert_endpoint::<${endpointName(route)}>();`);
  }
  out.push("    }");
  out.push("}");
  out.push("");
  return out.join("\n");
}

/** Render `src/generated/resources.rs`. */
function renderResources(routes) {
  const out = [...HEADER];
  out.push("//! Generated namespace wrappers for both clients.");
  out.push("//!");
  out.push("//! The blocking module is compiled by the `blocking` feature and the async");
  out.push("//! module by the `async` feature; the request types in `endpoints` are shared.");
  out.push("");

  const buildTree = (prefix) => {
    const root = { methods: [], children: new Map() };
    for (const route of routes) {
      let node = root;
      for (const segment of route.namespace) {
        if (!node.children.has(segment)) {
          node.children.set(segment, { methods: [], children: new Map() });
        }
        node = node.children.get(segment);
      }
      node.methods.push(route);
    }
    return root;
  };

  const renderTree = (node, indent, asyncMode, clientPath) => {
    const lines = [];
    for (const [segment, child] of [...node.children.entries()].sort()) {
      lines.push(`${indent}/// \`${segment}\` resources.`);
      lines.push(`${indent}pub mod ${snake(segment)} {`);
      lines.push(...renderTree(child, `${indent}    `, asyncMode, clientPath));
      lines.push(`${indent}}`);
      lines.push("");
    }
    for (const route of [...node.methods].sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const args = route.pathParams.map((name) => `${field(name)}: &str`);
      if (route.query) args.push(`query: crate::endpoints::${queryTypeName(route)}`);
      if (route.body) args.push(`body: ${route.body.replace("crate::types::", "crate::types::")}`);
      const ctorArgs = route.pathParams.map((name) => field(name));
      if (route.body) ctorArgs.push("body");
      let ctor = `crate::endpoints::${endpointName(route)}::new(${ctorArgs.join(", ")})`;
      if (route.query) ctor += ".with_query(query)";
      let expression;
      if (route.responseType === "ChatStream") {
        expression = `client.stream(&${ctor})`;
      } else {
        expression = `client.call(&${ctor})`;
      }
      if (asyncMode && route.responseType !== "ChatStream") expression += ".await";
      const ret =
        route.responseType === "ChatStream"
          ? "crate::stream::ChatStream"
          : route.responseType === "()" || route.responseType === "String"
            ? route.responseType
            : route.responseSchemas.length
              ? `crate::endpoints::${route.responseType}`
              : `crate::types::${route.responseType}`;
      if (route.summary) lines.push(...doc(route.summary, indent).trimEnd().split("\n"));
      lines.push(`${indent}pub ${asyncMode ? "async " : ""}fn ${snake(route.name)}(${["client: &crate::" + clientPath + "::Client", ...args].join(", ")}) -> crate::Result<${ret}> {`);
      lines.push(`${indent}    ${expression}`);
      lines.push(`${indent}}`);
      lines.push("");
    }
    return lines;
  };

  out.push("#[cfg(feature = \"blocking\")]");
  out.push("pub mod blocking_api {");
  out.push("    //! Typed blocking calls: one function per contract route.");
  out.push("");
  out.push(...renderTree(buildTree(), "    ", false, "blocking"));
  out.push("}");
  out.push("");
  out.push("#[cfg(feature = \"async\")]");
  out.push("pub mod async_api {");
  out.push("    //! Typed async calls: one function per contract route.");
  out.push("");
  out.push(...renderTree(buildTree(), "    ", true, "asynchronous"));
  out.push("}");
  out.push("");
  return out.join("\n");
}

/** Render every artifact, keyed by path relative to the crate. */
function render(spec, hash) {
  const routes = deriveRoutes(spec);
  return {
    "src/generated/types.rs": formatRust(renderTypes(spec)),
    "src/generated/routes.rs": formatRust(renderRoutes(spec, routes, hash)),
    "src/generated/endpoints.rs": formatRust(renderEndpoints(routes)),
    "src/generated/resources.rs": formatRust(renderResources(routes)),
  };
}

/** Format one generated source file with the repository's rustfmt. */
function formatRust(source) {
  const result = spawnSync(
    "rustfmt",
    ["--edition", "2024", "--emit", "stdout", "--quiet"],
    { input: source, encoding: "utf8" },
  );
  if (result.error || result.status !== 0) {
    const reason = result.error?.message ?? result.stderr ?? "unknown error";
    throw new Error(`rustfmt failed on a generated artifact: ${reason}`);
  }
  return result.stdout;
}

/** Program entry point: write every artifact, or check them with `--check`. */
function main(argv) {
  const check = argv.includes("--check");
  const bytes = readFileSync(CONTRACT);
  const hash = createHash("sha256").update(bytes).digest("hex");
  const spec = JSON.parse(bytes.toString("utf8"));
  const files = render(spec, hash);
  let stale = 0;
  for (const [relative, content] of Object.entries(files)) {
    const target = join(CRATE, relative);
    if (check) {
      let current = null;
      try {
        current = readFileSync(target, "utf8");
      } catch {
        current = null;
      }
      if (current !== content) {
        stale += 1;
        process.stderr.write(`stale generated artifact: ${relative}\n`);
      }
      continue;
    }
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, content);
  }
  if (check) {
    if (stale) {
      process.stderr.write("run `node scripts/generate.mjs` from sdk/rust to regenerate\n");
      process.exit(1);
    }
    process.stdout.write("sdk/rust generated artifacts are current\n");
  }
  process.stdout.write(`contract ${spec.info.version}: ${deriveRoutes(spec).length} routes, ${Object.keys(spec.components.schemas).length} schemas\n`);
}

main(process.argv.slice(2));
