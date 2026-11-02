//! Chatrooms and the session relay: cursor paging and the live stream.

import assert from "node:assert/strict";
import { test } from "node:test";

import { ChatStream, TidepoolClient, iterateActivity, iterateChatMessages, iterateRelayEvents } from "../src/index.js";
import { parseChatFrame } from "../src/stream.js";
import { loadFixture, RecordedServer } from "./support/recorded-server.mjs";

const BASE = "https://pool.example.com";

/** A client over one fixture. */
function clientFor(fixtureName, options = {}) {
  const server = new RecordedServer(loadFixture(fixtureName).steps);
  const client = new TidepoolClient({ baseUrl: BASE, token: "jwt", fetch: server.fetch, ...options });
  return { client, server };
}

/** A WebSocket double that records sends and replays scripted frames. */
class FakeWebSocket {
  constructor(url) {
    this.url = url;
    this.sent = [];
    this.closed = null;
    this.listeners = new Map();
    FakeWebSocket.instances.push(this);
  }

  static instances = [];

  addEventListener(type, handler) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(handler);
  }

  emit(type, event = {}) {
    for (const handler of this.listeners.get(type) ?? []) handler(event);
  }

  send(data) {
    this.sent.push(data);
  }

  close(code, reason) {
    this.closed = { code, reason };
    this.emit("close", { code, reason });
  }

  /** Deliver one scripted frame. */
  message(frame) {
    this.emit("message", { data: typeof frame === "string" ? frame : JSON.stringify(frame) });
  }
}

test("chat history iterates every page and stops at next=null", async () => {
  const { client, server } = clientFor("chat-pages");
  const bodies = [];
  for await (const message of iterateChatMessages(client, "room_01", { after: 0, limit: 2 })) {
    bodies.push(`${message.sequence}:${message.body}`);
  }
  assert.deepEqual(bodies, ["1:first", "2:second", "3:third"]);
  assert.deepEqual(
    server.calls.map((call) => call.query),
    [
      { after: "0", limit: "2" },
      { after: "2", limit: "2" },
    ],
  );
});

test("chat history honours maxPages as a safety stop", async () => {
  const { client, server } = clientFor("chat-pages");
  const seen = [];
  for await (const message of iterateChatMessages(client, "room_01", { limit: 2, maxPages: 1 })) {
    seen.push(message.sequence);
  }
  assert.deepEqual(seen, [1, 2]);
  assert.equal(server.calls.length, 1);
});

test("relay backfill iterates every page in sequence order", async () => {
  const { client } = clientFor("relay-pages");
  const kinds = [];
  for await (const event of iterateRelayEvents(client, "ses_01", { after: 0, limit: 2 })) {
    kinds.push(event.kind);
  }
  assert.deepEqual(kinds, ["started", "output", "stopped"]);
});

test("activity iterates while hasMore and follows nextCursor", async () => {
  const { client, server } = clientFor("activity-pages");
  const ids = [];
  for await (const entry of iterateActivity(client, { limit: 2 })) {
    ids.push(entry.id);
  }
  assert.deepEqual(ids, ["act_01", "act_02", "act_03"]);
  assert.deepEqual(server.calls.map((call) => call.query.cursor), [undefined, "c2"]);
  assert.equal(server.calls[1].query.limit, "2");
});

test("the live stream opens on the ws scheme with the token and resume cursor", () => {
  FakeWebSocket.instances = [];
  const client = new TidepoolClient({
    baseUrl: "https://pool.example.com/",
    token: "jwt-live",
    fetch: async () => new Response(null, { status: 204 }),
    WebSocket: FakeWebSocket,
  });

  const stream = client.chatrooms.stream("room_01", { after: 42 });
  assert.ok(stream instanceof ChatStream);
  assert.equal(FakeWebSocket.instances.length, 1);
  const socket = FakeWebSocket.instances[0];
  assert.equal(
    socket.url,
    "wss://pool.example.com/v1/chatrooms/room_01/stream?token=jwt-live&after=42",
  );
  assert.equal(stream.url, socket.url);
});

test("the stream delivers frames, tracks the cursor and ends on close", async () => {
  FakeWebSocket.instances = [];
  const frames = [];
  const client = new TidepoolClient({
    baseUrl: "http://localhost:8080",
    token: "jwt-live",
    fetch: async () => new Response(null, { status: 204 }),
    WebSocket: FakeWebSocket,
  });

  const stream = client.chatrooms.stream("room_01", { after: 0, onFrame: (frame) => frames.push(frame.type) });
  const socket = FakeWebSocket.instances[0];
  socket.emit("open");

  const seen = [];
  const drained = (async () => {
    for await (const frame of stream.frames()) seen.push(frame.type);
  })();

  socket.message({ type: "chat.history", catchUp: true, messages: [{ sequence: 1 }, { sequence: 2 }] });
  socket.message({ type: "chat.current", catchUp: false, cursor: 2 });
  socket.message({ type: "chat.message", catchUp: false, message: { sequence: 3, body: "hi" } });
  socket.message("not json");
  socket.close();

  await drained;
  assert.deepEqual(seen, ["chat.history", "chat.current", "chat.message", "unknown"]);
  assert.deepEqual(frames, ["chat.history", "chat.current", "chat.message", "unknown"]);
  assert.equal(stream.cursor, 3);
  assert.equal(stream.done, true);
  assert.equal(socket.url, "ws://localhost:8080/v1/chatrooms/room_01/stream?token=jwt-live&after=0");
});

test("chat.error closes the stream", () => {
  FakeWebSocket.instances = [];
  const client = new TidepoolClient({
    baseUrl: BASE,
    token: "jwt",
    fetch: async () => new Response(null, { status: 204 }),
    WebSocket: FakeWebSocket,
  });
  const stream = client.chatrooms.stream("room_01");
  const socket = FakeWebSocket.instances[0];
  socket.message({ type: "chat.error", message: "catch-up failed" });
  assert.equal(stream.done, true);
});

test("a stream needs a token and a WebSocket implementation", () => {
  const client = new TidepoolClient({ baseUrl: BASE, fetch: async () => new Response(null, { status: 204 }) });
  assert.throws(() => client.chatrooms.stream("room_01"), /token is required/);

  const withToken = new TidepoolClient({
    baseUrl: BASE,
    token: "jwt",
    fetch: async () => new Response(null, { status: 204 }),
  });
  const original = globalThis.WebSocket;
  delete globalThis.WebSocket;
  try {
    assert.throws(() => withToken.chatrooms.stream("room_01"), /no WebSocket implementation/);
  } finally {
    globalThis.WebSocket = original;
  }
});

test("parseChatFrame never throws", () => {
  assert.deepEqual(parseChatFrame('{"type":"chat.current","cursor":4}'), { type: "chat.current", cursor: 4 });
  assert.deepEqual(parseChatFrame("not json"), { type: "unknown", raw: "not json" });
  assert.deepEqual(parseChatFrame("null"), { type: "unknown", raw: null });
  assert.deepEqual(parseChatFrame(new Uint8Array([1])), { type: "binary", data: new Uint8Array([1]) });
});
