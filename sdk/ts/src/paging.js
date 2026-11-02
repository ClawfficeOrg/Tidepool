//! Cursor paging helpers over the contract's page shapes.
//!
//! The hub returns `next` (a sequence cursor) for chat and relay history and
//! `nextCursor`/`hasMore` for activity. These iterators follow the cursor until
//! the server stops offering one, so callers never hand-roll the loop.

/**
 * Iterate a room's messages in sequence order.
 *
 * @param {import("./client.js").TidepoolClient} client
 * @param {string} room
 * @param {{ after?: number, limit?: number, maxPages?: number, request?: Record<string, unknown> }} [options]
 */
export async function* iterateChatMessages(client, room, options = {}) {
  let after = options.after;
  let pages = 0;
  for (;;) {
    const page = await client.chatrooms.messages(room, {
      ...(options.request ?? {}),
      query: { after, limit: options.limit },
    });
    for (const message of page.messages ?? []) yield message;
    if (typeof page.next !== "number") return;
    after = page.next;
    pages += 1;
    if (options.maxPages !== undefined && pages >= options.maxPages) return;
  }
}

/**
 * Iterate a session's relayed events in sequence order.
 *
 * @param {import("./client.js").TidepoolClient} client
 * @param {string} session
 * @param {{ after?: number, limit?: number, maxPages?: number, request?: Record<string, unknown> }} [options]
 */
export async function* iterateRelayEvents(client, session, options = {}) {
  let after = options.after;
  let pages = 0;
  for (;;) {
    const page = await client.relay.events(session, {
      ...(options.request ?? {}),
      query: { after, limit: options.limit },
    });
    for (const event of page.events ?? []) yield event;
    if (typeof page.next !== "number") return;
    after = page.next;
    pages += 1;
    if (options.maxPages !== undefined && pages >= options.maxPages) return;
  }
}

/**
 * Iterate the activity feed, following `nextCursor` while `hasMore` is true.
 *
 * @param {import("./client.js").TidepoolClient} client
 * @param {{ cursor?: string, limit?: number, project?: string, provider?: string, trigger?: string, status?: string, kind?: string, maxPages?: number, request?: Record<string, unknown> }} [options]
 */
export async function* iterateActivity(client, options = {}) {
  let cursor = options.cursor;
  let pages = 0;
  for (;;) {
    const page = await client.activity.list({
      ...(options.request ?? {}),
      query: {
        cursor,
        limit: options.limit,
        project: options.project,
        provider: options.provider,
        trigger: options.trigger,
        status: options.status,
        kind: options.kind,
      },
    });
    for (const entry of page.items ?? []) yield entry;
    if (!page.hasMore || typeof page.nextCursor !== "string") return;
    cursor = page.nextCursor;
    pages += 1;
    if (options.maxPages !== undefined && pages >= options.maxPages) return;
  }
}
