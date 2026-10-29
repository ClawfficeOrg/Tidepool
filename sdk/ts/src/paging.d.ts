import type { TidepoolClient } from "./client.js";
import type { ActivityEntry, MessageView, RelayEvent } from "./generated/types.js";

/** Options shared by the cursor iterators. */
export interface PagingOptions {
  /** Exclusive starting cursor. */
  after?: number;
  /** Page size the server should use. */
  limit?: number;
  /** Stop after this many follow-up pages, even if the server has more. */
  maxPages?: number;
  /** Extra per-call options (requestId, signal, headers, retry). */
  request?: Record<string, unknown>;
}

/** Options for the activity iterator (string cursor). */
export interface ActivityPagingOptions {
  cursor?: string;
  limit?: number;
  project?: string;
  provider?: string;
  trigger?: string;
  status?: string;
  kind?: string;
  maxPages?: number;
  request?: Record<string, unknown>;
}

/** Iterate a room's messages in sequence order. */
export declare function iterateChatMessages(
  client: TidepoolClient,
  room: string,
  options?: PagingOptions,
): AsyncGenerator<MessageView, void, unknown>;

/** Iterate a session's relayed events in sequence order. */
export declare function iterateRelayEvents(
  client: TidepoolClient,
  session: string,
  options?: PagingOptions,
): AsyncGenerator<RelayEvent, void, unknown>;

/** Iterate the activity feed, following `nextCursor` while `hasMore` is true. */
export declare function iterateActivity(
  client: TidepoolClient,
  options?: ActivityPagingOptions,
): AsyncGenerator<ActivityEntry, void, unknown>;
