//! A recorded HTTP transport for the unit tests.
//!
//! Each fixture step is one recorded request/response pair, replayed in order.
//! The transport records every request it served and fails loudly on an
//! unexpected call, so a test that drifts from the contract fixtures fails
//! rather than silently passing. No sockets, no ports, no daemons.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const fixturesRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "fixtures");

/**
 * Load one recorded fixture file.
 *
 * @param {string} name file name without `.json`
 */
export function loadFixture(name) {
  return JSON.parse(readFileSync(join(fixturesRoot, `${name}.json`), "utf8"));
}

/**
 * One recorded HTTP exchange.
 *
 * @typedef {{
 *   request: { method: string, path: string, query?: Record<string, string>, body?: unknown },
 *   response: { status?: number, headers?: Record<string, string>, body?: unknown, text?: string },
 *   times?: number,
 * }} Step
 */

/** A transport that replays recorded exchanges. */
export class RecordedServer {
  /**
   * @param {Step[]} steps
   */
  constructor(steps) {
    /** Every request the client actually sent. */
    this.calls = [];
    this.#queue = steps.map((step) => ({ step, remaining: step.times ?? 1 }));
  }

  /** @type {Array<{ step: Step, remaining: number }>} */
  #queue;

  /** The `fetch` implementation to hand to the client. */
  get fetch() {
    return (url, init = {}) => this.#respond(String(url), init);
  }

  /** Steps that were never used; a test asserts this is empty. */
  get unmatched() {
    return this.#queue.filter((entry) => entry.remaining > 0);
  }

  /** Requests grouped by path, for query assertions. */
  callsTo(path) {
    return this.calls.filter((call) => call.path === path);
  }

  async #respond(url, init) {
    const parsed = new URL(url);
    const method = (init.method ?? "GET").toUpperCase();
    const rawBody = init.body;
    const body = typeof rawBody === "string" ? safeParse(rawBody) : rawBody;
    const call = {
      method,
      url,
      path: parsed.pathname,
      query: Object.fromEntries(parsed.searchParams.entries()),
      headers: normaliseHeaders(init.headers),
      body,
    };
    this.calls.push(call);

    const index = this.#queue.findIndex(
      (entry) =>
        entry.remaining > 0 &&
        entry.step.request.method.toUpperCase() === method &&
        entry.step.request.path === parsed.pathname,
    );
    if (index === -1) {
      throw new Error(`recorded server: unexpected ${method} ${parsed.pathname} (${url})`);
    }
    const entry = this.#queue[index];
    entry.remaining -= 1;
    const step = entry.step;

    const headers = { ...(step.response.headers ?? {}) };
    let payload = step.response.text;
    if (payload === undefined && step.response.body !== undefined) {
      payload = JSON.stringify(step.response.body);
      if (!Object.keys(headers).some((key) => key.toLowerCase() === "content-type")) {
        headers["content-type"] = "application/json";
      }
    }
    return new Response(payload ?? null, {
      status: step.response.status ?? 200,
      headers,
    });
  }
}

/** Normalise fetch headers to a plain object. */
function normaliseHeaders(headers) {
  if (!headers) return {};
  if (typeof headers.forEach === "function") {
    /** @type {Record<string, string>} */
    const record = {};
    headers.forEach((value, key) => {
      record[key.toLowerCase()] = value;
    });
    return record;
  }
  return Object.fromEntries(Object.entries(headers).map(([key, value]) => [key.toLowerCase(), String(value)]));
}

function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
