/**
 * Chatrooms: the model behind the room list, the member list, the history and
 * the live stream.
 *
 * The hub's chatroom surface is deliberately small: the rooms a member belongs
 * to, the members of one room, cursor-paged history and one WebSocket per room
 * (`src/api/chatrooms.rs`, `src/sockets/chatroom_ws.rs`). This module owns what
 * the browser page does with that surface:
 *
 * - **history is merged by the server's id and ordered by the server's
 *   sequence.** A page, a live frame and a reconnect replay may all carry the
 *   same message; the page shows it once, under the id the pool minted.
 * - **older messages load through the cursor without duplication.** The
 *   contract pages forward with `after`, so an older page is read by stepping
 *   the cursor back one page at a time and keeping only messages below the
 *   oldest one already held. Sequences may have gaps; a short read steps back
 *   again instead of assuming the page was complete.
 * - **catch-up is a state, not a tone of voice.** Cached history renders while
 *   `catchingUp` is true, and the page never says "current" until a fetch (and,
 *   when a socket is open, the server's `chat.current` frame) confirms it. A
 *   dropped socket puts the page back into catch-up.
 * - **a person and an agent are distinguishable without colour.** The author
 *   descriptor carries a kind, a word and a glyph, and the markup exposes
 *   `data-author-kind` and an accessible label, so a monochrome or
 *   colour-blind reader still tells them apart.
 *
 * Everything that can be decided without a network is a pure function here, so
 * the merge, the backward cursor walk, the author descriptor and the catch-up
 * labels are tested without a DOM, a socket or a pool. The controller takes a
 * transport and a stream factory by injection for the same reason.
 */

import type {
  ChatFrame,
  HistoryResponse,
  MessageView,
  Principal,
  RoomMemberView,
  RoomView,
  SendMessageBody,
  TidepoolClient,
} from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, type PoolConfig } from './api/pool';

// ── constants ──────────────────────────────────────────────────────────────

/** How many messages one history read asks for. */
export const ROOM_PAGE_SIZE = 50;

/** How many messages one room keeps in its local cache. */
export const ROOM_CACHE_LIMIT = 200;

/** How many backward cursor steps one older-page read may take. */
export const MAX_BACKFILL_STEPS = 100;

/** How long the page waits before retrying a dropped stream. */
export const RECONNECT_DELAY_MS = 2_000;

/** The banner while the page is behind the room. */
export const CATCH_UP_NOTICE =
  'Catching up — showing cached history until the pool confirms this room is current.';

/** The banner once a fetch has confirmed currentness. */
export const CURRENT_NOTICE = 'Current — every message up to now is shown.';

/** The banner while the live stream is down. */
export const RECONNECT_NOTICE =
  'Reconnecting — the live stream dropped, so the cached history is not current until the pool confirms it.';

/** The sentence shown when a viewer may not post. */
export const VIEWER_CANNOT_POST =
  'This room is read-only for you: you joined as a viewer, so the pool will not accept a message.';

/** The sentence shown when there is no cached history. */
export const NO_CACHED_HISTORY =
  'No cached history for this room yet; the first page arrives from the pool.';

// ── author descriptors ─────────────────────────────────────────────────────

/** What kind of principal authored a message or joined a room. */
export type PrincipalKindLabel = 'user' | 'session' | 'unknown';

/** How one author is presented, by more than colour. */
export interface AuthorDescriptor {
  /** The principal kind, or `unknown` when the author was deleted. */
  kind: PrincipalKindLabel;
  /** The word a person reads. */
  kindLabel: string;
  /** A glyph that differs in shape, not only in colour. */
  glyph: string;
  /** The principal id, when there is one. */
  id: string | null;
  /** One accessible label combining the kind and the id. */
  label: string;
}

/** The descriptor for a missing author. */
export const UNKNOWN_AUTHOR: AuthorDescriptor = {
  kind: 'unknown',
  kindLabel: 'Unknown author',
  glyph: '?',
  id: null,
  label: 'Unknown author',
};

/**
 * Describe one principal.
 *
 * A person is a filled circle and the word "Person"; an agent session is a
 * diamond and the words "Agent session". The shapes differ, the words differ
 * and the accessible label differs, so colour is never the only signal.
 */
export function describeAuthor(author: Principal | null | undefined): AuthorDescriptor {
  if (!author || typeof author.id !== 'string' || author.id.trim() === '') {
    return UNKNOWN_AUTHOR;
  }
  if (author.kind === 'session') {
    return {
      kind: 'session',
      kindLabel: 'Agent session',
      glyph: '◆',
      id: author.id,
      label: `Agent session ${author.id}`,
    };
  }
  if (author.kind === 'user') {
    return {
      kind: 'user',
      kindLabel: 'Person',
      glyph: '●',
      id: author.id,
      label: `Person ${author.id}`,
    };
  }
  return {
    kind: 'unknown',
    kindLabel: 'Unknown author',
    glyph: '?',
    id: author.id,
    label: `Unknown author ${author.id}`,
  };
}

/** The descriptor for one room member. */
export function describeMember(member: RoomMemberView): AuthorDescriptor {
  return describeAuthor(member.principal);
}

/** True when this member is an agent session rather than a person. */
export function memberIsSession(member: RoomMemberView): boolean {
  return member.principal.kind === 'session';
}

// ── merging ────────────────────────────────────────────────────────────────

/**
 * Merge messages into one list, keyed by the server's id.
 *
 * A message that arrives twice (a page overlapping a live frame, a reconnect
 * replay, an older page that touches the window) appears once. The newer copy
 * wins, because the server is the source of truth for a body. The list is
 * ordered by `sequence`, with the id as a deterministic tie-break; timestamps
 * are never compared.
 */
export function mergeMessages(
  existing: readonly MessageView[],
  incoming: readonly MessageView[],
): MessageView[] {
  const byId = new Map<string, MessageView>();
  for (const message of existing) byId.set(message.id, message);
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort(compareMessages);
}

/** Order two messages by the server's sequence, then by id. */
export function compareMessages(left: MessageView, right: MessageView): number {
  if (left.sequence !== right.sequence) return left.sequence - right.sequence;
  return left.id.localeCompare(right.id);
}

/** The highest sequence in a list, which is the cursor for "newer". */
export function headCursor(messages: readonly MessageView[]): number | null {
  let head: number | null = null;
  for (const message of messages) {
    if (head === null || message.sequence > head) head = message.sequence;
  }
  return head;
}

/** The lowest sequence in a list, which is the cursor for "older". */
export function tailCursor(messages: readonly MessageView[]): number | null {
  let tail: number | null = null;
  for (const message of messages) {
    if (tail === null || message.sequence < tail) tail = message.sequence;
  }
  return tail;
}

// ── transport ──────────────────────────────────────────────────────────────

/** One room stream, reduced to what the page needs. */
export interface RoomStream {
  /** Subscribe to every frame; returns an unsubscribe function. */
  onFrame(listener: (frame: ChatFrame) => void): () => void;
  /** Subscribe to the close event; returns an unsubscribe function. */
  onClose(listener: () => void): () => void;
  /** Close the socket. Safe to call twice. */
  close(): void;
}

/** Open one room's live stream after a sequence cursor. */
export type OpenRoomStream = (
  room: string,
  options: { after: number | null },
) => RoomStream;

/** The pool calls the chatroom page makes. */
export interface RoomsTransport {
  /** The rooms this member belongs to. */
  listRooms(): Promise<RoomView[]>;
  /** The members of one room. */
  members(room: string): Promise<RoomMemberView[]>;
  /** One page of history after a cursor. */
  history(room: string, query: { after?: number; limit?: number }): Promise<HistoryResponse>;
  /** Post one message. */
  send(room: string, body: SendMessageBody): Promise<MessageView>;
}

/** The room list is an array on the wire, though the generated type is bare. */
function asRoomList(value: unknown): RoomView[] {
  return Array.isArray(value) ? (value as RoomView[]) : [];
}

/** The member list is an array on the wire, though the generated type is bare. */
function asMemberList(value: unknown): RoomMemberView[] {
  return Array.isArray(value) ? (value as RoomMemberView[]) : [];
}

/** Build the transport from the typed SDK client. */
export function roomsTransport(client: TidepoolClient): RoomsTransport {
  return {
    listRooms: async () =>
      asRoomList(await client.api.chatrooms.list()),
    members: async (room) =>
      asMemberList(await client.api.chatrooms.members(room)),
    history: (room, query) => client.api.chatrooms.messages(room, { query }),
    send: (room, body) => client.api.chatrooms.send(room, body),
  };
}

/**
 * Build the live-stream factory from the typed SDK client.
 *
 * The token is carried in the query string because a browser WebSocket cannot
 * set an `Authorization` header; the SDK does that from its own token.
 */
export function roomStreamFactory(client: TidepoolClient): OpenRoomStream {
  return (room, options) => {
    const query: { after?: number } = {};
    if (options.after !== null) query.after = options.after;
    const chat = client.api.chatrooms.stream(room, { query });
    const closeListeners = new Set<() => void>();
    void chat.closed.then(() => {
      for (const listener of [...closeListeners]) listener();
    });
    return {
      onFrame: (listener) => chat.on('*', listener),
      onClose: (listener) => {
        closeListeners.add(listener);
        return () => closeListeners.delete(listener);
      },
      close: () => chat.close(),
    };
  };
}

// ── history pages ──────────────────────────────────────────────────────────

/** One older page and whether anything older still exists. */
export interface OlderPage {
  /** Messages immediately older than the requested cursor, ascending. */
  messages: MessageView[];
  /** True when a further older page may exist. */
  hasOlder: boolean;
}

/**
 * Read the page immediately older than `before`.
 *
 * The contract only reads forward from `after`, so the walk steps the cursor
 * back by one page at a time and asks for `(cursor - pageSize, …)`. Everything
 * at or above `before` is discarded; the rest is accumulated ascending. A short
 * read is not proof that the beginning was reached: with gaps in the sequence,
 * the walk steps back again until a full page is collected or the room's start
 * is read. The returned page is the newest `pageSize` messages below `before`,
 * so merging it can never duplicate a message the page already holds.
 */
export async function olderMessagesPage(
  transport: RoomsTransport,
  room: string,
  before: number,
  pageSize: number = ROOM_PAGE_SIZE,
): Promise<OlderPage> {
  const size = Math.max(1, Math.floor(pageSize));
  let cursor = before;
  let collected: MessageView[] = [];
  let reachedStart = false;

  for (let step = 0; step < MAX_BACKFILL_STEPS; step += 1) {
    if (cursor <= 0) {
      reachedStart = true;
      break;
    }
    const window = Math.max(0, cursor - size);
    const page = await transport.history(room, { after: window, limit: size });
    const below = page.messages
      .filter((message) => message.sequence < cursor)
      .sort(compareMessages);
    if (below.length === 0) {
      if (window === 0) {
        reachedStart = true;
        break;
      }
      cursor = window;
      continue;
    }
    collected = [...below, ...collected];
    cursor = below[0]?.sequence ?? window;
    if (window === 0) {
      // Reading from the very beginning returned the room's first message, so
      // there is nothing older even when its sequence is not 1.
      reachedStart = true;
    }
    if (collected.length >= size) break;
  }

  const trimmed =
    collected.length > size ? collected.slice(collected.length - size) : collected;
  const hasOlder = !reachedStart || collected.length > size;
  return { messages: trimmed, hasOlder };
}

// ── cache ──────────────────────────────────────────────────────────────────

/** A place to keep a room's recent messages between views and reconnects. */
export interface RoomCache {
  /** Read the cached messages for a room, oldest first. */
  read(room: string): MessageView[];
  /** Replace the cache for a room. */
  write(room: string, messages: readonly MessageView[]): void;
}

/** An in-memory cache; one per controller unless a caller supplies another. */
export function memoryRoomCache(limit: number = ROOM_CACHE_LIMIT): RoomCache {
  const rooms = new Map<string, MessageView[]>();
  return {
    read: (room) => [...(rooms.get(room) ?? [])],
    write: (room, messages) => {
      const bounded = [...messages].sort(compareMessages).slice(-Math.max(1, limit));
      rooms.set(room, bounded);
    },
  };
}

// ── view state ─────────────────────────────────────────────────────────────

/** The live connection state the page renders. */
export type RoomPresence = 'idle' | 'connecting' | 'live' | 'reconnecting' | 'offline';

/** Everything the chatroom page renders. */
export interface RoomsViewState {
  /** The room list's load state. */
  phase: 'idle' | 'loading' | 'ready' | 'error';
  /** The rooms this member belongs to. */
  rooms: RoomView[];
  /** A failure loading the room list. */
  roomsProblem: PoolProblem | null;
  /** The room the reader opened. */
  selectedRoomId: string | null;
  /** The selected room's projection. */
  room: RoomView | null;
  /** The room's members. */
  members: RoomMemberView[];
  /** The member list's load state. */
  membersPhase: 'idle' | 'loading' | 'ready' | 'error';
  /** A failure loading the room's members. */
  membersProblem: PoolProblem | null;
  /** The messages on screen, ordered by the server's sequence. */
  messages: MessageView[];
  /** The history's load state. */
  historyPhase: 'idle' | 'loading' | 'ready' | 'error';
  /** A failure loading history. */
  historyProblem: PoolProblem | null;
  /** True while an older page is being read. */
  loadingOlder: boolean;
  /** True when a further older page may exist. */
  hasOlder: boolean;
  /** A failure reading an older page. */
  olderProblem: PoolProblem | null;
  /** True until a fetch (and a live `chat.current`) confirms currentness. */
  catchingUp: boolean;
  /** True only once currentness has been confirmed. */
  current: boolean;
  /** The live connection state. */
  presence: RoomPresence;
  /** The principals the pool reports watching the room now. */
  watchers: string[];
  /** True when the live stream dropped and may be reopened. */
  reconnectNeeded: boolean;
  /** Where a send is. */
  sendPhase: 'idle' | 'sending' | 'sent' | 'failed';
  /** A failure from a send. */
  sendProblem: PoolProblem | null;
  /** When the pool last confirmed the room as current. */
  lastSyncedAt: string | null;
  /** True while any pool call is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyRoomsState(): RoomsViewState {
  return {
    phase: 'idle',
    rooms: [],
    roomsProblem: null,
    selectedRoomId: null,
    room: null,
    members: [],
    membersPhase: 'idle',
    membersProblem: null,
    messages: [],
    historyPhase: 'idle',
    historyProblem: null,
    loadingOlder: false,
    hasOlder: false,
    olderProblem: null,
    catchingUp: false,
    current: false,
    presence: 'idle',
    watchers: [],
    reconnectNeeded: false,
    sendPhase: 'idle',
    sendProblem: null,
    lastSyncedAt: null,
    busy: false,
  };
}

/** True when the page can load from this pool config and token. */
export function canLoadRooms(
  config: PoolConfig,
  token: string | null | undefined,
): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}

/** How the catch-up banner should be labelled. */
export interface CurrentnessBanner {
  /** Stable status for markup and tests. */
  status: 'catching-up' | 'reconnecting' | 'current' | 'offline';
  /** The sentence a person reads. */
  text: string;
}

/**
 * The currentness banner.
 *
 * `current` is only ever the answer when the controller has confirmed it and
 * the live stream has not dropped. Every other path — including a cached first
 * render, a fetch in flight, a `chat.catchUp` frame and a closed socket — says
 * catching up or reconnecting, so the page cannot pretend to be current.
 */
export function currentnessBanner(
  state: Pick<RoomsViewState, 'catchingUp' | 'current' | 'presence'>,
): CurrentnessBanner {
  if (state.catchingUp || !state.current) {
    return state.presence === 'reconnecting'
      ? { status: 'reconnecting', text: RECONNECT_NOTICE }
      : { status: 'catching-up', text: CATCH_UP_NOTICE };
  }
  if (state.presence === 'offline') {
    return { status: 'offline', text: RECONNECT_NOTICE };
  }
  return { status: 'current', text: CURRENT_NOTICE };
}

/** True only when the page has confirmed it is current. */
export function isCurrent(
  state: Pick<RoomsViewState, 'catchingUp' | 'current' | 'presence'>,
): boolean {
  return currentnessBanner(state).status === 'current';
}

/** The sentence describing who is watching a room now. */
export function presenceSentence(watchers: readonly string[]): string {
  if (watchers.length === 0) return 'No one else is watching this room right now.';
  if (watchers.length === 1) return '1 watcher: ' + (watchers[0] ?? 'unknown');
  return `${watchers.length} watchers: ${watchers.join(', ')}`;
}

/** True when a scroll position is close enough to the top to ask for older. */
export function shouldLoadOlder(
  scrollTop: number,
  hasOlder: boolean,
  loadingOlder: boolean,
  threshold = 48,
): boolean {
  return hasOlder && !loadingOlder && Number.isFinite(scrollTop) && scrollTop <= threshold;
}

/** The membership role the signed-in person has in a room, when known. */
export function roomRoleFor(
  members: readonly RoomMemberView[],
  viewerId: string | null | undefined,
): RoomMemberView['role'] | null {
  const id = typeof viewerId === 'string' ? viewerId.trim() : '';
  if (id === '') return null;
  const mine = members.find(
    (member) => member.principal.kind === 'user' && member.principal.id === id,
  );
  return mine?.role ?? null;
}

/**
 * True when the page should offer the composer.
 *
 * Until the member list names the viewer, the composer is offered and the pool
 * remains the authority; once the list says `viewer`, it is withdrawn with an
 * explanation rather than accepting a message that would come back 403.
 */
export function canPostInRoom(
  members: readonly RoomMemberView[],
  viewerId: string | null | undefined,
): boolean {
  return roomRoleFor(members, viewerId) !== 'viewer';
}

/** The role word a person reads. */
export function memberRoleLabel(role: RoomMemberView['role']): string {
  switch (role) {
    case 'owner':
      return 'Owner';
    case 'member':
      return 'Member';
    case 'viewer':
      return 'Viewer';
    default:
      return 'Member';
  }
}

/** The visibility word a person reads. */
export function visibilityLabel(visibility: RoomView['visibility']): string {
  switch (visibility) {
    case 'private':
      return 'Private';
    case 'unlisted':
      return 'Unlisted';
    case 'public':
      return 'Public';
    default:
      return 'Unknown';
  }
}

// ── scheduler ──────────────────────────────────────────────────────────────

/** One timer the controller can schedule; injectable so tests are deterministic. */
export interface RoomsScheduler {
  /** Run `callback` after `delayMs`, returning a handle for `cancel`. */
  schedule(callback: () => void | Promise<unknown>, delayMs: number): unknown;
  /** Cancel a handle returned by `schedule`. */
  cancel(handle: unknown): void;
}

/** The default scheduler: the browser's or Node's `setTimeout`. */
export const DEFAULT_ROOMS_SCHEDULER: RoomsScheduler = {
  schedule: (callback, delayMs) => setTimeout(() => void callback(), delayMs),
  cancel: (handle) => {
    if (handle !== null && handle !== undefined) {
      clearTimeout(handle as ReturnType<typeof setTimeout>);
    }
  },
};

// ── controller ─────────────────────────────────────────────────────────────

/** How the controller is built. */
export interface RoomsControllerOptions {
  /** The pool calls. */
  transport: RoomsTransport;
  /** Opens the live stream; omitted in a headerless or offline view. */
  openStream?: OpenRoomStream | null;
  /** Cached history, so a reconnect renders immediately. */
  cache?: RoomCache;
  /** A resumed or preloaded state. */
  initial?: RoomsViewState;
  /** Where the reconnect timer comes from. */
  scheduler?: RoomsScheduler;
  /** The clock, so `lastSyncedAt` is deterministic in tests. */
  now?: () => number;
  /** Called on every state change, so a page can render cached and live states. */
  onChange?: (state: RoomsViewState) => void;
}

/** The chatroom page's controller. */
export class RoomsController {
  private readonly transport: RoomsTransport;
  private readonly openStream: OpenRoomStream | null;
  private readonly cache: RoomCache;
  private readonly scheduler: RoomsScheduler;
  private readonly now: () => number;
  private readonly onChange: ((state: RoomsViewState) => void) | null;
  private storedState: RoomsViewState;
  /** Guards a stale room from writing into the selected room. */
  private selectionToken = 0;
  /** The open socket, when one is live. */
  private stream: RoomStream | null = null;
  /** Unsubscribe from the open socket. */
  private unsubscribe: (() => void) | null = null;
  /** Unsubscribe from the open socket's close event. */
  private closeUnsubscribe: (() => void) | null = null;
  /** The pending reconnect timer, when one is scheduled. */
  private reconnectHandle: unknown = null;
  /** True after dispose; no frame or timer may touch state afterwards. */
  private disposed = false;

  constructor(options: RoomsControllerOptions) {
    this.transport = options.transport;
    this.openStream = options.openStream ?? null;
    this.cache = options.cache ?? memoryRoomCache();
    this.scheduler = options.scheduler ?? DEFAULT_ROOMS_SCHEDULER;
    this.now = options.now ?? Date.now;
    this.onChange = options.onChange ?? null;
    this.storedState = options.initial ?? emptyRoomsState();
  }

  /** The current state; every write notifies `onChange`. */
  private get state(): RoomsViewState {
    return this.storedState;
  }

  private set state(next: RoomsViewState) {
    this.storedState = next;
    this.onChange?.(next);
  }

  /** The current state, without touching the pool. */
  snapshot(): RoomsViewState {
    return this.state;
  }

  /** Load the rooms this member belongs to. */
  async loadRooms(): Promise<RoomsViewState> {
    this.state = {
      ...this.state,
      phase: this.state.phase === 'ready' ? 'ready' : 'loading',
      roomsProblem: null,
      busy: true,
    };
    try {
      const rooms = await this.transport.listRooms();
      this.state = {
        ...this.state,
        phase: 'ready',
        rooms,
        roomsProblem: null,
        busy: false,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        phase: 'error',
        roomsProblem: presentProblem(error),
        busy: false,
      };
    }
    return this.state;
  }

  /**
   * Open one room: show its cached history, read the members, fetch the
   * history up to the head, then open the live stream.
   *
   * `catchingUp` is true from the first cached render. Without a stream the
   * fetch's completion is the currentness signal. With a stream, the server's
   * `chat.current` frame is required as well, so a message that lands between
   * the fetch and the socket cannot be claimed as already shown.
   */
  async openRoom(roomId: string): Promise<RoomsViewState> {
    const id = roomId.trim();
    if (id === '') return this.state;
    this.disposeStream();
    this.selectionToken += 1;
    const token = this.selectionToken;
    const room = this.state.rooms.find((candidate) => candidate.id === id) ?? null;
    const cached = this.cache.read(id);
    this.state = {
      ...this.state,
      selectedRoomId: id,
      room,
      messages: cached,
      members: [],
      membersPhase: 'loading',
      membersProblem: null,
      historyPhase: cached.length > 0 ? 'ready' : 'loading',
      historyProblem: null,
      loadingOlder: false,
      hasOlder: tailCursor(cached) !== null && (tailCursor(cached) ?? 1) > 1,
      olderProblem: null,
      catchingUp: true,
      current: false,
      presence: this.openStream ? 'connecting' : 'idle',
      watchers: [],
      reconnectNeeded: false,
      sendPhase: 'idle',
      sendProblem: null,
      lastSyncedAt: null,
      busy: true,
    };

    const members = this.transport.members(id);
    const history = this.catchUp(id, headCursor(cached), token);
    const [memberResult] = await Promise.allSettled([members, history]);
    if (token !== this.selectionToken || this.disposed) return this.state;

    if (memberResult.status === 'fulfilled') {
      this.state = {
        ...this.state,
        members: memberResult.value,
        membersPhase: 'ready',
        membersProblem: null,
      };
    } else if (this.state.membersPhase === 'loading') {
      this.state = {
        ...this.state,
        membersPhase: 'error',
        membersProblem: presentProblem(memberResult.reason),
      };
    }

    if (this.state.historyPhase !== 'error') {
      this.state = { ...this.state, historyPhase: 'ready' };
    }
    this.state = {
      ...this.state,
      messages: mergeMessages([], this.state.messages),
      hasOlder:
        this.state.hasOlder ||
        (tailCursor(this.state.messages) !== null && (tailCursor(this.state.messages) ?? 1) > 1),
      busy: false,
    };
    this.cache.write(id, this.state.messages);

    if (this.openStream && !this.disposed) {
      this.openLive(id, token);
    } else if (!this.state.historyProblem) {
      this.state = {
        ...this.state,
        catchingUp: false,
        current: true,
        presence: 'idle',
        reconnectNeeded: false,
        lastSyncedAt: new Date(this.now()).toISOString(),
      };
    }
    return this.state;
  }

  /**
   * Read forward to the room's head, page by page.
   *
   * An empty page is the contract's currentness signal. The pages are merged as
   * they arrive, so the page fills in even while it is still labelled
   * catching up.
   */
  private async catchUp(room: string, from: number | null, token: number): Promise<void> {
    let cursor = from;
    for (let step = 0; step < MAX_BACKFILL_STEPS; step += 1) {
      const query: { after?: number; limit?: number } = { limit: ROOM_PAGE_SIZE };
      if (cursor !== null) query.after = cursor;
      let page: HistoryResponse;
      try {
        page = await this.transport.history(room, query);
      } catch (error) {
        if (token !== this.selectionToken || this.disposed) return;
        this.state = {
          ...this.state,
          historyPhase: 'error',
          historyProblem: presentProblem(error),
          catchingUp: true,
          current: false,
        };
        return;
      }
      if (token !== this.selectionToken || this.disposed) return;
      if (page.messages.length === 0) return;
      this.state = {
        ...this.state,
        messages: mergeMessages(this.state.messages, page.messages),
        historyPhase: 'ready',
        historyProblem: null,
      };
      this.cache.write(room, this.state.messages);
      const last = page.messages[page.messages.length - 1];
      const next = page.next ?? null;
      if (next === null || (last !== undefined && next === cursor)) return;
      cursor = next ?? last?.sequence ?? null;
      if (cursor === null) return;
    }
  }

  /** Open the live socket and adopt its frames. */
  private openLive(room: string, token: number): void {
    if (!this.openStream || this.disposed) return;
    this.disposeStream();
    this.state = {
      ...this.state,
      catchingUp: true,
      current: false,
      presence: 'connecting',
      reconnectNeeded: false,
    };
    let stream: RoomStream;
    try {
      stream = this.openStream(room, { after: headCursor(this.state.messages) });
    } catch {
      this.state = {
        ...this.state,
        catchingUp: true,
        current: false,
        presence: 'reconnecting',
        reconnectNeeded: true,
      };
      this.scheduleReconnect();
      return;
    }
    this.stream = stream;
    this.unsubscribe = stream.onFrame((frame) => this.handleFrame(room, token, frame));
    this.closeUnsubscribe = stream.onClose(() => {
      // An intentional close nulls `this.stream` before closing, so only a
      // server-initiated drop reaches here.
      if (this.stream === stream) this.markClosed(room, token);
    });
  }

  /** Adopt one live frame. */
  private handleFrame(room: string, token: number, frame: ChatFrame): void {
    if (this.disposed || token !== this.selectionToken) return;
    if (this.state.selectedRoomId !== room) return;
    switch (frame.type) {
      case 'chat.history': {
        const history = (frame as { messages?: MessageView[] }).messages ?? [];
        this.state = {
          ...this.state,
          messages: mergeMessages(this.state.messages, history),
          catchingUp: true,
          current: false,
          presence: 'live',
        };
        this.cache.write(room, this.state.messages);
        return;
      }
      case 'chat.current': {
        const cursor = (frame as { cursor?: number | null }).cursor;
        this.state = {
          ...this.state,
          catchingUp: false,
          current: true,
          presence: 'live',
          reconnectNeeded: false,
          lastSyncedAt: new Date(this.now()).toISOString(),
        };
        this.cache.write(room, this.state.messages);
        void cursor;
        return;
      }
      case 'chat.message': {
        const message = (frame as { message?: MessageView }).message;
        if (!message) return;
        this.state = {
          ...this.state,
          messages: mergeMessages(this.state.messages, [message]),
          presence: 'live',
          lastSyncedAt: new Date(this.now()).toISOString(),
        };
        this.cache.write(room, this.state.messages);
        return;
      }
      case 'chat.presence': {
        const watchers = (frame as { watchers?: string[] }).watchers ?? [];
        this.state = {
          ...this.state,
          watchers: [...watchers].filter((watcher) => typeof watcher === 'string'),
          presence: 'live',
        };
        return;
      }
      case 'chat.catchUp': {
        this.state = { ...this.state, catchingUp: true, current: false };
        return;
      }
      case 'chat.error': {
        this.state = {
          ...this.state,
          catchingUp: true,
          current: false,
          presence: 'reconnecting',
          reconnectNeeded: true,
        };
        this.markClosed(room, token);
        return;
      }
      default:
        return;
    }
  }

  /** Record a dropped socket and put the page back into catch-up. */
  private markClosed(room: string, token: number): void {
    if (this.disposed || token !== this.selectionToken) return;
    if (this.state.selectedRoomId !== room) return;
    if (this.closeUnsubscribe) {
      this.closeUnsubscribe();
      this.closeUnsubscribe = null;
    }
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = null;
    }
    const stream = this.stream;
    this.stream = null;
    if (stream) stream.close();
    this.state = {
      ...this.state,
      catchingUp: true,
      current: false,
      presence: 'reconnecting',
      reconnectNeeded: true,
    };
    this.scheduleReconnect();
  }

  /** Schedule one reconnect attempt, replacing any pending one. */
  private scheduleReconnect(): void {
    if (this.disposed || this.reconnectHandle !== null) return;
    this.reconnectHandle = this.scheduler.schedule(() => {
      this.reconnectHandle = null;
      if (this.state.selectedRoomId !== null) this.reconnect();
    }, RECONNECT_DELAY_MS);
  }

  /** Narrow the state so a dropped socket is visible even without a frame. */
  noteStreamClosed(): RoomsViewState {
    const room = this.state.selectedRoomId;
    if (room === null) return this.state;
    this.markClosed(room, this.selectionToken);
    return this.state;
  }

  /**
   * Reopen the live stream from the last known cursor.
   *
   * Cached messages stay on screen and the banner stays on "catching up" until
   * the server sends `chat.current`; a reconnect never declares itself current
   * on its own.
   */
  reconnect(): RoomsViewState {
    const room = this.state.selectedRoomId;
    if (room === null || !this.openStream || this.disposed) return this.state;
    if (this.reconnectHandle !== null) {
      this.scheduler.cancel(this.reconnectHandle);
      this.reconnectHandle = null;
    }
    this.openLive(room, this.selectionToken);
    return this.state;
  }

  /** Read one older page and merge it into the window. */
  async loadOlder(): Promise<RoomsViewState> {
    const room = this.state.selectedRoomId;
    if (room === null || this.state.loadingOlder || !this.state.hasOlder) return this.state;
    const before = tailCursor(this.state.messages);
    if (before === null) {
      this.state = { ...this.state, hasOlder: false };
      return this.state;
    }
    const token = this.selectionToken;
    this.state = { ...this.state, loadingOlder: true, olderProblem: null };
    try {
      const page = await olderMessagesPage(this.transport, room, before, ROOM_PAGE_SIZE);
      if (token !== this.selectionToken || this.disposed) return this.state;
      this.state = {
        ...this.state,
        messages: mergeMessages(this.state.messages, page.messages),
        hasOlder: page.hasOlder && page.messages.length > 0,
        loadingOlder: false,
        olderProblem: null,
      };
      this.cache.write(room, this.state.messages);
    } catch (error) {
      if (token !== this.selectionToken || this.disposed) return this.state;
      this.state = {
        ...this.state,
        loadingOlder: false,
        olderProblem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Post one message and adopt the server's copy immediately. */
  async send(body: string): Promise<RoomsViewState> {
    const room = this.state.selectedRoomId;
    const text = body.trim();
    if (room === null || text === '') return this.state;
    this.state = { ...this.state, sendPhase: 'sending', sendProblem: null };
    try {
      const message = await this.transport.send(room, { body: text });
      this.state = {
        ...this.state,
        messages: mergeMessages(this.state.messages, [message]),
        sendPhase: 'sent',
        sendProblem: null,
      };
      this.cache.write(room, this.state.messages);
    } catch (error) {
      this.state = {
        ...this.state,
        sendPhase: 'failed',
        sendProblem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Close the room, its socket and any pending reconnect. */
  closeRoom(): RoomsViewState {
    this.disposeStream();
    this.selectionToken += 1;
    this.state = {
      ...this.state,
      selectedRoomId: null,
      room: null,
      members: [],
      membersPhase: 'idle',
      membersProblem: null,
      messages: [],
      historyPhase: 'idle',
      historyProblem: null,
      loadingOlder: false,
      hasOlder: false,
      olderProblem: null,
      catchingUp: false,
      current: false,
      presence: 'idle',
      watchers: [],
      reconnectNeeded: false,
      sendPhase: 'idle',
      sendProblem: null,
      lastSyncedAt: null,
      busy: false,
    };
    return this.state;
  }

  /** Close the socket and every timer; the controller is inert afterwards. */
  dispose(): void {
    this.disposed = true;
    this.disposeStream();
  }

  /** Close only the socket and its timer. */
  private disposeStream(): void {
    if (this.reconnectHandle !== null) {
      this.scheduler.cancel(this.reconnectHandle);
      this.reconnectHandle = null;
    }
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = null;
    }
    if (this.closeUnsubscribe) {
      this.closeUnsubscribe();
      this.closeUnsubscribe = null;
    }
    if (this.stream) {
      const stream = this.stream;
      this.stream = null;
      stream.close();
    }
  }
}
