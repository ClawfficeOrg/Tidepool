import type { MessageView, QueryValues } from "./generated/types.js";

/** One frame the server may send on a chatroom socket. */
export type ChatFrame =
  | { type: "chat.history"; catchUp: true; messages: MessageView[] }
  | { type: "chat.current"; catchUp: false; cursor: number | null }
  | { type: "chat.message"; catchUp: false; message: MessageView }
  | { type: "chat.presence"; catchUp: false; watchers: string[] }
  | { type: "chat.catchUp"; catchUp: true }
  | { type: "chat.error"; message: string }
  | { type: "unknown"; raw?: unknown }
  | { type: "binary"; data: unknown }
  | { type: string; [key: string]: unknown };

/** Bounded retry policy; a 429 is never retried indefinitely. */
export interface RetryOptions {
  /** Maximum retries after the first attempt. */
  maxRetries: number;
  /** First exponential delay when no `Retry-After` is sent. */
  baseDelayMs: number;
  /** Ceiling for exponential delays. */
  maxDelayMs: number;
  /** Add up to 50% jitter to exponential delays. */
  jitter: boolean;
  /** A longer `Retry-After` is surfaced instead of slept through. */
  maxRetryAfterMs: number;
}

/** Per-call options accepted by every generated method. */
export interface RequestOptions<Q extends QueryValues = QueryValues> {
  /** Query-string parameters; typed per route where the handler reads them. */
  query?: Q;
  /** Correlation id sent as `x-request-id`. */
  requestId?: string;
  /** Extra headers for this call only. */
  headers?: Record<string, string>;
  /** Abort the call (also cancels any pending retry sleep in flight). */
  signal?: AbortSignal;
  /** Client-side timeout for this call, in milliseconds. */
  timeoutMs?: number;
  /** Override the client retry policy, or `false` to disable retrying. */
  retry?: Partial<RetryOptions> | false;
}

/** One live chatroom connection. */
export declare class ChatStream {
  constructor(
    socket: WebSocket,
    options?: { url?: string; onFrame?: (frame: ChatFrame) => void; onClose?: () => void },
  );
  /** The underlying socket. */
  readonly socket: WebSocket;
  /** Absolute `ws://` or `wss://` URL. */
  readonly url: string;
  /** Highest sequence delivered, suitable as the next `after` cursor. */
  cursor: number | null;
  /** True once the socket closed. */
  done: boolean;
  /** Resolves when the socket opens. */
  readonly ready: Promise<ChatStream>;
  /** Resolves when the socket closes. */
  readonly closed: Promise<ChatStream>;
  /** Subscribe to one frame type (or `"*"`); returns an unsubscribe function. */
  on(type: string, handler: (frame: ChatFrame) => void): () => void;
  /** Iterate frames until the socket closes. */
  frames(): AsyncGenerator<ChatFrame, void, unknown>;
  /** Send one client frame. */
  send(frame: string | Record<string, unknown>): void;
  /** Close the socket; safe to call twice. */
  close(code?: number, reason?: string): void;
}

/** Open a chatroom stream over WebSocket. */
export declare function openChatStream(options: {
  url: string;
  WebSocketImpl?: typeof WebSocket | null;
  onFrame?: (frame: ChatFrame) => void;
  onClose?: () => void;
}): ChatStream;

/** Parse one socket payload into a frame; never throws. */
export declare function parseChatFrame(data: unknown): ChatFrame;

/** Frame types the server sends. */
export declare const CHAT_FRAME_TYPES: readonly string[];
