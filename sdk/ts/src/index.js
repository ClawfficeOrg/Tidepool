//! Tidepool TypeScript SDK — browser-safe entry point.
//!
//! Fetch-only transport, no Node built-ins. The Node entry (`node.js`) adds
//! token-file loading on top of this module.

import { TidepoolClient } from "./client.js";

export { TidepoolClient, DEFAULT_RETRY, TidepoolUsageError, delayFor } from "./client.js";
export { TidepoolError, NetworkError, ProblemError, isProblemError, normaliseProblem, retryAfterMs } from "./problem.js";
export { CHAT_FRAME_TYPES, ChatStream, openChatStream, parseChatFrame } from "./stream.js";
export { iterateActivity, iterateChatMessages, iterateRelayEvents } from "./paging.js";
export { CONTRACT_VERSION, OPERATIONS, ROLES, ROUTES, SCOPES, route } from "./generated/routes.js";
export { buildResources } from "./generated/resources.js";

/**
 * Build a client. The same call works in a browser, in Node 20+ and in a
 * worker; pass `fetch` to use a custom transport.
 *
 * @param {ConstructorParameters<typeof TidepoolClient>[0]} options
 */
export function createClient(options) {
  return new TidepoolClient(options);
}

/**
 * A fresh idempotency key for `runs.dispatch` (`deliveryKey`) or
 * `hosts.createExecution` (`execution_id`).
 *
 * @returns {string}
 */
export function deliveryKey() {
  const crypto = globalThis.crypto;
  if (crypto && typeof crypto.randomUUID === "function") return crypto.randomUUID();
  return `key-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
