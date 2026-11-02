/**
 * Two browsers in one room, across the real TypeScript SDK.
 *
 * Each "browser" is a `RoomsController` holding its own typed SDK client,
 * `fetch` transport and WebSocket implementation. Both talk to one in-memory
 * pool: a room with 120 messages, two members and a fan-out hub. The test walks
 * the task's end-to-end case — both clients open the same room, one sends a
 * message, the other sees it live under the same server id, presence lists both
 * watchers, and the client that started from a cached tail pages backwards
 * through the cursor without duplicates.
 *
 * Nothing here starts a server: the SDK's URLs, auth header, query strings and
 * JSON shapes are the real ones.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type {
  HistoryResponse,
  MessageView,
  RoomMemberView,
  RoomView,
} from '@clawffice/tidepool-sdk';
import RoomsPage from './RoomsPage.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  RoomsController,
  currentnessBanner,
  roomStreamFactory,
  roomsTransport,
  type RoomCache,
} from '$lib/rooms';

const BASE_URL = 'https://pool.example.com';
const TOKEN_A = 'token-a';
const TOKEN_B = 'token-b';

const ROOM: RoomView = {
  id: 'room-builds',
  name: 'builds',
  visibility: 'private',
  origin_pool: null,
  created_at: '2026-10-20T09:00:00.000Z',
  updated_at: '2026-10-29T05:00:00.000Z',
};

/** One message, authored by whoever the fake member id names. */
function message(sequence: number, author = 'user-ada', body?: string): MessageView {
  return {
    id: `server-${sequence}`,
    author: { kind: 'user', id: author },
    body: body ?? `Message ${sequence}`,
    project: 'project-a',
    reply_to: null,
    created_at: `2026-10-29T05:${String(Math.floor(sequence / 60)).padStart(2, '0')}:${String(
      sequence % 60,
    ).padStart(2, '0')}.000Z`,
    sequence,
  };
}

/** One member. */
const MEMBERS: RoomMemberView[] = [
  {
    id: 'member-ada',
    principal: { kind: 'user', id: 'user-ada' },
    role: 'owner',
    joined_at: '2026-10-20T09:00:00.000Z',
  },
  {
    id: 'member-agent',
    principal: { kind: 'session', id: 'session-7' },
    role: 'member',
    joined_at: '2026-10-21T09:00:00.000Z',
  },
];

/** What both browser clients share. */
interface Hub {
  messages: MessageView[];
  sockets: Set<FakeSocket>;
  nextSequence: number;
}

/** The hub the next socket constructor attaches to. */
let activeHub: Hub | null = null;

/** The watcher labels the hub reports, sorted. */
function watchersOf(hub: Hub): string[] {
  return [...new Set([...hub.sockets].map((socket) => socket.token))].sort();
}

/** A WebSocket with the shape `openChatStream` uses. */
type FakeListener = (event: { data?: unknown }) => void;

class FakeSocket {
  readonly token: string;
  readonly after: number;
  readonly room: string;
  readonly listeners = new Map<string, Set<FakeListener>>();
  open = false;

  constructor(url: string | URL) {
    const parsed = new URL(String(url));
    const segments = parsed.pathname.split('/').filter((part) => part !== '');
    this.room = segments[3] ?? '';
    this.token = parsed.searchParams.get('token') ?? '';
    this.after = Number(parsed.searchParams.get('after') ?? '0') || 0;
    if (activeHub === null) throw new Error('no fake hub is active');
    const hub = activeHub;
    queueMicrotask(() => this.start(hub));
  }

  addEventListener(type: string, listener: FakeListener): void {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type)?.add(listener);
  }

  removeEventListener(type: string, listener: FakeListener): void {
    this.listeners.get(type)?.delete(listener);
  }

  send(data: string): void {
    let frame: unknown;
    try {
      frame = JSON.parse(data);
    } catch {
      return;
    }
    if (
      frame !== null &&
      typeof frame === 'object' &&
      (frame as { type?: unknown }).type === 'chat.fetch'
    ) {
      this.push({ type: 'chat.catchUp', catchUp: true });
    }
  }

  close(): void {
    if (!this.open) return;
    this.open = false;
    const hub = activeHub;
    if (hub) {
      hub.sockets.delete(this);
      broadcastPresence(hub);
    }
    this.fire('close', {});
  }

  /** Push one server frame into the client. */
  push(frame: unknown): void {
    if (!this.open) return;
    this.fire('message', { data: JSON.stringify(frame) });
  }

  private start(hub: Hub): void {
    this.open = true;
    hub.sockets.add(this);
    const backlog = hub.messages.filter((item) => item.sequence > this.after);
    if (backlog.length > 0) {
      this.push({ type: 'chat.history', catchUp: true, messages: backlog });
    }
    this.push({
      type: 'chat.current',
      catchUp: false,
      cursor: hub.messages[hub.messages.length - 1]?.sequence ?? null,
    });
    broadcastPresence(hub);
  }

  private fire(type: string, event: { data?: unknown }): void {
    for (const listener of [...(this.listeners.get(type) ?? [])]) listener(event);
  }
}

/** Tell every socket who is watching. */
function broadcastPresence(hub: Hub): void {
  const watchers = watchersOf(hub);
  for (const socket of hub.sockets) {
    socket.push({ type: 'chat.presence', catchUp: false, watchers });
  }
}

/** A JSON response. */
function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response. */
function problem(status: number, code: string): Response {
  return new Response(
    JSON.stringify({ status, code, title: code, detail: code, requestId: 'req_e2e_rooms' }),
    {
      status,
      headers: { 'content-type': 'application/problem+json', 'x-request-id': 'req_e2e_rooms' },
    },
  );
}

/** The fake pool's `fetch`, mirroring the contract's routes and shapes. */
function fakeFetch(hub: Hub): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string'
        ? new URL(input)
        : input instanceof URL
          ? input
          : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;
    const bearer = new Headers(init?.headers).get('authorization') ?? '';
    const token = bearer.startsWith('Bearer ') ? bearer.slice('Bearer '.length) : '';
    if (token !== TOKEN_A && token !== TOKEN_B) {
      return problem(401, 'unauthorized');
    }

    if (method === 'GET' && path === '/v1/chatrooms') {
      return json([ROOM]);
    }

    const membersRoute = /^\/v1\/chatrooms\/([^/]+)\/members$/.exec(path);
    if (method === 'GET' && membersRoute) {
      return json(MEMBERS);
    }

    const messagesRoute = /^\/v1\/chatrooms\/([^/]+)\/messages$/.exec(path);
    if (messagesRoute) {
      if (method === 'GET') {
        const after = Number(url.searchParams.get('after') ?? '0') || 0;
        const limit = Number(url.searchParams.get('limit') ?? '50') || 50;
        const remaining = hub.messages.filter((item) => item.sequence > after);
        const page = remaining.slice(0, limit);
        const last = page[page.length - 1];
        const response: HistoryResponse = {
          messages: page,
          next: remaining.length > limit && last ? last.sequence : null,
        };
        return json(response);
      }
      if (method === 'POST') {
        const raw = typeof init?.body === 'string' ? init.body : '{}';
        const parsed: unknown = JSON.parse(raw);
        const body =
          parsed !== null && typeof parsed === 'object'
            ? ((parsed as { body?: unknown }).body ?? '')
            : '';
        const author = token === TOKEN_A ? 'user-ada' : 'user-grace';
        const created = message(hub.nextSequence, author, String(body));
        hub.nextSequence += 1;
        hub.messages.push(created);
        for (const socket of hub.sockets) {
          socket.push({ type: 'chat.message', catchUp: false, message: created });
        }
        return json(created, 201);
      }
    }

    return problem(404, 'not_found');
  };
}

/** A client for one browser. */
function browser(hub: Hub, token: string, cache?: RoomCache) {
  const client = createClient({
    baseUrl: BASE_URL,
    token,
    fetch: fakeFetch(hub),
    WebSocket: FakeSocket as unknown as typeof WebSocket,
  });
  return {
    client,
    controller: new RoomsController({
      transport: roomsTransport(client),
      openStream: roomStreamFactory(client),
      cache,
      now: () => Date.parse('2026-10-29T06:00:00.000Z'),
    }),
  };
}

/** Let queued microtasks and socket opens run. */
async function flush(): Promise<void> {
  for (let turn = 0; turn < 6; turn += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

/** A cache that starts one browser at the last 60 messages. */
function tailCache(all: MessageView[]): RoomCache {
  const tail = all.slice(-60);
  return {
    read: (room) => (room === ROOM.id ? [...tail] : []),
    write: () => undefined,
  };
}

describe('two browsers in one room', () => {
  it('sees the same server ids, presence and currentness on both sides', async () => {
    const hub: Hub = {
      messages: Array.from({ length: 120 }, (_, index) => message(index + 1)),
      sockets: new Set(),
      nextSequence: 121,
    };
    activeHub = hub;
    try {
      const a = browser(hub, TOKEN_A, tailCache(hub.messages));
      const b = browser(hub, TOKEN_B);

      await a.controller.loadRooms();
      await b.controller.loadRooms();
      await a.controller.openRoom(ROOM.id);
      await b.controller.openRoom(ROOM.id);
      await flush();

      // Both clients are in the room and the pool has confirmed them current.
      expect(currentnessBanner(a.controller.snapshot()).status).toBe('current');
      expect(currentnessBanner(b.controller.snapshot()).status).toBe('current');
      expect(a.controller.snapshot().watchers).toEqual([TOKEN_A, TOKEN_B]);
      expect(b.controller.snapshot().watchers).toEqual([TOKEN_A, TOKEN_B]);
      // Presence never substituted for history.
      expect(b.controller.snapshot().messages).toHaveLength(120);

      // A sends; B receives the live frame under the server's id.
      await a.controller.send('hello from browser A');
      await flush();
      const fromA = a.controller.snapshot().messages;
      const fromB = b.controller.snapshot().messages;
      const sent = fromA[fromA.length - 1];
      expect(sent?.body).toBe('hello from browser A');
      expect(sent?.sequence).toBe(121);
      expect(sent?.id).toBe('server-121');
      expect(fromB).toHaveLength(121);
      expect(fromB[fromB.length - 1]?.id).toBe('server-121');
      expect(fromB[fromB.length - 1]?.body).toBe('hello from browser A');

      // The browser that started from a cached tail pages back through the
      // cursor and still holds every message once.
      expect(a.controller.snapshot().messages).toHaveLength(61);
      let older = a.controller.snapshot();
      for (let page = 0; page < 5 && older.hasOlder; page += 1) {
        older = await a.controller.loadOlder();
      }
      expect(older.messages).toHaveLength(121);
      expect(new Set(older.messages.map((item) => item.id)).size).toBe(121);
      expect(older.messages[0]?.sequence).toBe(1);
      expect(older.hasOlder).toBe(false);
      expect(currentnessBanner(older).status).toBe('current');

      // What browser B's reader would see, rendered from its own state.
      const rendered = renderComponent(RoomsPage, {
        pool: { baseUrl: BASE_URL },
        token: TOKEN_B,
        viewerId: 'user-grace',
        initial: b.controller.snapshot(),
      });
      expect(rendered.text).toContain('hello from browser A');
      expect(rendered.text).toContain('Person');
      expect(rendered.text).toContain('Agent session');
      expect(rendered.html).toContain('data-catch-up-status="current"');

      a.controller.dispose();
      b.controller.dispose();
    } finally {
      activeHub = null;
    }
  });
});
