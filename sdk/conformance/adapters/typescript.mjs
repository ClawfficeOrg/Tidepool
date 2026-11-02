#!/usr/bin/env node
//! Conformance adapter for the TypeScript SDK (`@clawffice/tidepool-sdk`).
//!
//! Thin translation only: the suite's generic commands become SDK calls and
//! the answers are normalised. No scenario lives here.
//!
//! Protocol: one JSON request per line on stdin, one JSON response per line on
//! stdout. Requests are `{id, command, params}`; responses are `{id, ok: true,
//! result}` or `{id, ok: false, error}`. Diagnostics go to stderr.

import { createInterface } from "node:readline";

import {
  CONTRACT_VERSION,
  ProblemError,
  ROUTES,
  TidepoolClient,
  iterateActivity,
  iterateChatMessages,
  iterateRelayEvents,
  parseChatFrame,
} from "../../ts/src/index.js";

const MAX_PAGES = 50;
const BASE_URL = process.env.TIDEPOOL_CONFORMANCE_BASE_URL;

class ProtocolError extends Error {}

/** Build one client for the step's ambient credentials. */
function buildClient(auth = {}) {
  return new TidepoolClient({
    baseUrl: BASE_URL,
    token: auth.token ?? null,
    apiKey: auth.apiKey ?? null,
    hostCredential: auth.hostCredential ?? null,
    timeoutMs: 60_000,
  });
}

/** Call one operation by id and normalise success or problem details. */
async function call(params) {
  const client = buildClient(params.auth);
  const body = params.body === null ? undefined : params.body;
  try {
    const result = await client.request(params.operation, {
      pathParams: params.pathParams ?? {},
      query: params.query ?? {},
      body,
      requestId: params.requestId,
      headers: params.headers ?? {},
    });
    return { outcome: "ok", body: result === undefined ? null : result };
  } catch (error) {
    if (!(error instanceof ProblemError)) throw error;
    return {
      outcome: "error",
      status: error.status,
      code: error.code,
      title: error.title,
      detail: error.detail,
      type: error.type,
      request_id: error.requestId,
      attempts: error.attempts,
      retry_after_ms: error.retryAfterMs,
    };
  }
}

/** Return the TypeScript SDK's route metadata for one operation id. */
function describe(params) {
  const route = ROUTES[params.operation];
  if (!route) throw new ProtocolError(`operation ${params.operation} is not in the contract`);
  // `query` is a `{name: type}` map when the route takes query parameters and
  // null when it does not; the suite wants the names in order.
  const query = route.query ?? null;
  const queryParams = Array.isArray(query) ? [...query] : Object.keys(query ?? {});
  return {
    operation_id: route.operationId,
    method: route.method,
    path: route.path,
    auth: route.auth,
    scope: route.scope,
    role: route.role,
    surface: route.surface,
    path_params: [...route.pathParams],
    query_params: queryParams,
  };
}

/** Walk one of the SDK's cursor iterators and normalise the items. */
async function page(params) {
  const client = buildClient(params.auth);
  const options = {
    after: params.after,
    limit: params.limit,
    maxPages: MAX_PAGES,
  };
  const items = [];
  if (params.kind === "chat") {
    for await (const message of iterateChatMessages(client, params.id, options)) {
      items.push({ sequence: message.sequence, body: message.body });
    }
  } else if (params.kind === "relay") {
    for await (const event of iterateRelayEvents(client, params.id, options)) {
      items.push({ sequence: event.sequence, kind: event.kind });
    }
  } else if (params.kind === "activity") {
    for await (const entry of iterateActivity(client, {
      cursor: params.cursor,
      limit: params.limit,
      maxPages: MAX_PAGES,
    })) {
      items.push({ kind: entry.kind, status: entry.status });
    }
  } else {
    throw new ProtocolError(`unknown paging kind ${params.kind}`);
  }
  return { items };
}

/** Minimal socket shape: the SDK builds the handle, no connection is opened. */
class RecordingSocket {
  constructor(url) {
    this.url = url;
  }

  addEventListener() {}

  send() {}

  close() {}
}

/** Build the SDK's live-stream handle without opening a socket. */
function stream(params) {
  const client = buildClient(params.auth);
  const handle = client._stream(
    params.operation,
    {
      token: params.auth?.token ?? null,
      after: params.after,
      webSocket: RecordingSocket,
    },
    params.pathParams ?? {},
  );
  const url = new URL(handle.url);
  const token = params.auth?.token ?? null;
  const afterParam = url.searchParams.get("after");
  return {
    scheme: url.protocol.replace(":", ""),
    after: afterParam === null ? null : Number(afterParam),
    token_present: url.searchParams.has("token"),
    token_matches: url.searchParams.get("token") === token,
    after_param: afterParam,
    path: url.pathname,
  };
}

/** Parse socket payloads with the SDK's frame parser. */
function frames(params) {
  const parsed = [];
  for (const text of params.texts ?? []) {
    const frame = parseChatFrame(text);
    const sequences = [];
    if (Array.isArray(frame.messages)) {
      for (const message of frame.messages) {
        if (typeof message?.sequence === "number") sequences.push(message.sequence);
      }
    }
    if (typeof frame.message?.sequence === "number") sequences.push(frame.message.sequence);
    parsed.push({
      kind: frame.type,
      catch_up: frame.catchUp === true,
      cursor: typeof frame.cursor === "number" ? frame.cursor : null,
      sequences,
      code: typeof frame.code === "string" ? frame.code : null,
    });
  }
  return { frames: parsed };
}

/** Dispatch one protocol request to the SDK adapter function. */
async function handle(request) {
  const params = request.params ?? {};
  switch (request.command) {
    case "contract":
      return {
        contract_version: CONTRACT_VERSION,
        operation_count: Object.keys(ROUTES).length,
      };
    case "describe":
      return describe(params);
    case "call":
      return call(params);
    case "page":
      return page(params);
    case "stream":
      return stream(params);
    case "frames":
      return frames(params);
    default:
      throw new ProtocolError(`unknown command ${request.command}`);
  }
}

async function main() {
  if (!BASE_URL) {
    console.error("typescript adapter: TIDEPOOL_CONFORMANCE_BASE_URL is not set");
    process.exit(2);
  }
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line.trim()) continue;
    let request;
    try {
      request = JSON.parse(line);
    } catch {
      console.error("typescript adapter: request is not JSON");
      process.exit(2);
    }
    let response;
    try {
      response = { id: request.id, ok: true, result: await handle(request) };
    } catch (error) {
      const kind = error?.name ?? "Error";
      const message =
        error instanceof ProtocolError ? error.message : "the SDK raised an unexpected error";
      if (!(error instanceof ProtocolError)) {
        console.error(`typescript adapter: ${kind}`);
      }
      response = { id: request.id, ok: false, error: { type: kind, message } };
    }
    process.stdout.write(`${JSON.stringify(response)}\n`);
  }
}

await main();
