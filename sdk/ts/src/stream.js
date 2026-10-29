//! Live chatroom stream over WebSocket.
//!
//! `GET /v1/chatrooms/{room}/stream?token=<jwt>&after=<sequence>` upgrades to a
//! WebSocket (`src/sockets/chatroom_ws.rs`). The socket first replays
//! `chat.history` frames for everything newer than `after`, then one
//! `chat.current` frame marks the end of catch-up; live `chat.message` and
//! `chat.presence` frames follow. `chat.error` closes the socket.
//!
//! Nothing here imports a Node built-in: `WebSocket` is a browser global and a
//! Node 22 global, and callers can inject an implementation (tests, older
//! runtimes) through `webSocket`.

/** Frame types the server sends on a chatroom socket. */
export const CHAT_FRAME_TYPES = Object.freeze([
  "chat.history",
  "chat.current",
  "chat.message",
  "chat.presence",
  "chat.catchUp",
  "chat.error",
]);

/**
 * Parse one socket payload into a frame. Non-JSON text becomes
 * `{ type: "unknown", raw }` so a caller never has to try/catch.
 *
 * @param {unknown} data
 * @returns {{ type: string, [key: string]: unknown }}
 */
export function parseChatFrame(data) {
  if (typeof data !== "string") return { type: "binary", data };
  try {
    const parsed = JSON.parse(data);
    if (parsed && typeof parsed === "object" && typeof parsed.type === "string") return parsed;
    return { type: "unknown", raw: parsed };
  } catch {
    return { type: "unknown", raw: data };
  }
}

/** One live chatroom connection. */
export class ChatStream {
  /**
   * @param {WebSocket} socket
   * @param {{ url?: string, onFrame?: (frame: any) => void, onClose?: () => void }} [options]
   */
  constructor(socket, options = {}) {
    /** The underlying socket (browser `WebSocket` shape). */
    this.socket = socket;
    /** Absolute `ws://` or `wss://` URL. */
    this.url = options.url ?? "";
    /** Last sequence delivered by the server, when one was seen. */
    this.cursor = null;
    /** True once the socket closed. */
    this.done = false;
    /** Highest `chat.current` cursor, i.e. the resume point after a drop. */
    this.#onFrame = options.onFrame;
    this.#onClose = options.onClose;
    this.#buffer = [];
    this.#waiters = [];
    this.ready = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    this.ready.catch(() => {});
    this.closed = new Promise((resolve) => {
      this.#resolveClosed = resolve;
    });
    this.#bind();
  }

  #buffer;
  #waiters;
  #handlers = new Map();
  #resolveReady;
  #rejectReady;
  #resolveClosed;
  #onFrame;
  #onClose;

  #bind() {
    const socket = this.socket;
    socket.addEventListener("open", () => {
      if (this.#resolveReady) this.#resolveReady(this);
    });
    socket.addEventListener("message", (event) => {
      const frame = parseChatFrame(event?.data);
      if (typeof frame.cursor === "number") this.cursor = frame.cursor;
      if (frame.type === "chat.message" && frame.message && typeof frame.message.sequence === "number") {
        this.cursor = frame.message.sequence;
      }
      if (typeof this.#onFrame === "function") this.#onFrame(frame);
      for (const handler of this.#handlers.get(frame.type) ?? []) handler(frame);
      for (const handler of this.#handlers.get("*") ?? []) handler(frame);
      if (frame.type === "chat.error") {
        this.#finish();
        return;
      }
      if (this.#waiters.length) this.#waiters.shift()({ value: frame, done: false });
      else this.#buffer.push(frame);
    });
    socket.addEventListener("error", (event) => {
      if (this.#rejectReady) this.#rejectReady(event);
    });
    socket.addEventListener("close", () => {
      this.#finish();
    });
  }

  #finish() {
    if (this.done) return;
    this.done = true;
    for (const waiter of this.#waiters.splice(0)) waiter({ value: undefined, done: true });
    if (this.#resolveClosed) this.#resolveClosed(this);
    if (typeof this.#onClose === "function") this.#onClose();
  }

  /**
   * Subscribe to one frame type, or `"*"` for all.
   *
   * @param {string} type
   * @param {(frame: any) => void} handler
   * @returns {() => void} unsubscribe
   */
  on(type, handler) {
    if (!this.#handlers.has(type)) this.#handlers.set(type, new Set());
    this.#handlers.get(type).add(handler);
    return () => this.#handlers.get(type)?.delete(handler);
  }

  /**
   * Iterate frames as they arrive. Ends when the socket closes.
   *
   * @returns {AsyncGenerator<any, void, unknown>}
   */
  async *frames() {
    for (;;) {
      if (this.#buffer.length) {
        yield this.#buffer.shift();
        continue;
      }
      if (this.done) return;
      const next = await new Promise((resolve) => this.#waiters.push(resolve));
      if (next.done) return;
      yield next.value;
    }
  }

  /** Send one client frame, e.g. `{ type: "chat.fetch" }` to pull history. */
  send(frame) {
    this.socket.send(typeof frame === "string" ? frame : JSON.stringify(frame));
  }

  /** Close the socket; safe to call twice. */
  close(code = 1000, reason = "client closed") {
    if (this.done) return;
    try {
      this.socket.close(code, reason);
    } finally {
      this.#finish();
    }
  }
}

/**
 * Open a chatroom stream.
 *
 * @param {{ url: string, WebSocketImpl?: typeof WebSocket | null, onFrame?: (frame: any) => void, onClose?: () => void }} options
 * @returns {ChatStream}
 */
export function openChatStream(options) {
  const Impl = options.WebSocketImpl ?? globalThis.WebSocket;
  if (typeof Impl !== "function") {
    throw new TypeError(
      "no WebSocket implementation available; pass options.webSocket or use Node 22+ / a browser",
    );
  }
  const socket = new Impl(options.url);
  return new ChatStream(socket, { url: options.url, onFrame: options.onFrame, onClose: options.onClose });
}
