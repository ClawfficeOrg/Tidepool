import type { ClientOptions, TidepoolClient } from "./client.js";

/** Build a client; identical in a browser, a worker and Node 20+. */
export declare function createClient(options: ClientOptions): TidepoolClient;

/** A fresh idempotency key for `deliveryKey` / `execution_id`. */
export declare function deliveryKey(): string;

export * from "./client.js";
export {
  CHAT_FRAME_TYPES,
  ChatStream,
  openChatStream,
  parseChatFrame,
} from "./stream.js";
export type { ChatFrame, RequestOptions, RetryOptions } from "./stream.js";
export {
  NetworkError,
  ProblemError,
  TidepoolError,
  isProblemError,
  normaliseProblem,
  retryAfterMs,
} from "./problem.js";
export type { ProblemDetails } from "./problem.js";
export { iterateActivity, iterateChatMessages, iterateRelayEvents } from "./paging.js";
export { CONTRACT_VERSION, OPERATIONS, ROLES, ROUTES, SCOPES, route } from "./generated/routes.js";
export { buildResources } from "./generated/resources.js";
export type * from "./generated/types.js";
