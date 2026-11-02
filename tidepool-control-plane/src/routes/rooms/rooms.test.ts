/**
 * The chatroom page's logic and markup, without a pool, a socket or a DOM.
 *
 * The pure functions own the merge, the backward cursor walk, the author
 * descriptor and the catch-up words. The controller tests inject a transport
 * and a fake stream, so "live" means the same frame callback a browser socket
 * would run. The server-rendered component tests assert what a person reads:
 * cached history under a catch-up banner, a live message appended under the
 * server's id, an agent session labelled as one, and a viewer's composer
 * withdrawn with a reason.
 */

import { describe, expect, it } from 'vitest';
import type {
  ChatFrame,
  MessageView,
  Principal,
  RoomMemberView,
  RoomView,
} from '@clawffice/tidepool-sdk';
import RoomsPage from './RoomsPage.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  CATCH_UP_NOTICE,
  CURRENT_NOTICE,
  RECONNECT_NOTICE,
  ROOM_PAGE_SIZE,
  RoomsController,
  VIEWER_CANNOT_POST,
  canPostInRoom,
  currentnessBanner,
  describeAuthor,
  emptyRoomsState,
  isCurrent,
  mergeMessages,
  olderMessagesPage,
  presenceSentence,
  roomRoleFor,
  shouldLoadOlder,
  tailCursor,
  type OpenRoomStream,
  type RoomCache,
  type RoomStream,
  type RoomsScheduler,
  type RoomsTransport,
  type RoomsViewState,
} from '$lib/rooms';

const POOL = { baseUrl: 'https://pool.example.com' };

const ROOM: RoomView = {
  id: 'room-builds',
  name: 'builds',
  visibility: 'private',
  origin_pool: null,
  created_at: '2026-10-20T09:00:00.000Z',
  updated_at: '2026-10-29T05:00:00.000Z',
};

const OTHER_ROOM: RoomView = {
  ...ROOM,
  id: 'room-deploys',
  name: 'deploys',
  visibility: 'public',
};

/** One message, with a contiguous sequence by default. */
function message(sequence: number, overrides: Partial<MessageView> = {}): MessageView {
  return {
    id: `message-${sequence}`,
    author: { kind: 'user', id: 'user-ada' },
    body: `Message ${sequence}`,
    project: 'project-a',
    reply_to: null,
    created_at: `2026-10-29T05:00:${String(sequence % 60).padStart(2, '0')}.000Z`,
    sequence,
    ...overrides,
  };
}

/** A run of messages, inclusive. */
function messages(from: number, to: number): MessageView[] {
  const out: MessageView[] = [];
  for (let sequence = from; sequence <= to; sequence += 1) out.push(message(sequence));
  return out;
}

/** One member. */
function member(
  id: string,
  kind: 'user' | 'session',
  role: RoomMemberView['role'] = 'member',
): RoomMemberView {
  return {
    id: `member-${id}`,
    principal: { kind, id } as Principal,
    role,
    joined_at: '2026-10-20T09:00:00.000Z',
  };
}

/** A view state with overrides. */
function stateWith(overrides: Partial<RoomsViewState> = {}): RoomsViewState {
  return { ...emptyRoomsState(), ...overrides };
}

/** A transport over an in-memory room, recording every history read. */
function fakeTransport(all: MessageView[], room: RoomView = ROOM): RoomsTransport & {
  reads: { after?: number; limit?: number }[];
  sent: string[];
} {
  const reads: { after?: number; limit?: number }[] = [];
  const sent: string[] = [];
  return {
    reads,
    sent,
    listRooms: () => Promise.resolve([room]),
    members: () =>
      Promise.resolve([member('user-ada', 'user', 'owner'), member('session-7', 'session')]),
    history: (_room, query) => {
      reads.push({ ...query });
      const after = query.after ?? 0;
      const limit = query.limit ?? ROOM_PAGE_SIZE;
      const page = all.filter((item) => item.sequence > after).slice(0, limit);
      const more = all.filter((item) => item.sequence > (page[page.length - 1]?.sequence ?? after));
      return Promise.resolve({
        messages: page,
        next: more.length > 0 ? (page[page.length - 1]?.sequence ?? null) : null,
      });
    },
    send: (_room, body) => {
      sent.push(body.body);
      const sequence = (all[all.length - 1]?.sequence ?? 0) + 1;
      const created = message(sequence, { id: `message-${sequence}`, body: body.body });
      all.push(created);
      return Promise.resolve(created);
    },
  };
}

/** One fake socket the controller can open, drop and reopen. */
class FakeStream implements RoomStream {
  readonly frames: ((frame: ChatFrame) => void)[] = [];
  readonly closes: (() => void)[] = [];
  closed = false;

  onFrame(listener: (frame: ChatFrame) => void): () => void {
    this.frames.push(listener);
    return () => {
      const index = this.frames.indexOf(listener);
      if (index >= 0) this.frames.splice(index, 1);
    };
  }

  onClose(listener: () => void): () => void {
    this.closes.push(listener);
    return () => {
      const index = this.closes.indexOf(listener);
      if (index >= 0) this.closes.splice(index, 1);
    };
  }

  emit(frame: ChatFrame): void {
    for (const listener of [...this.frames]) listener(frame);
  }

  drop(): void {
    if (this.closed) return;
    this.closed = true;
    for (const listener of [...this.closes]) listener();
  }

  close(): void {
    this.drop();
  }
}

/** A stream factory that hands out the sockets it created. */
function fakeStreams(): { opened: FakeStream[]; open: OpenRoomStream } {
  const opened: FakeStream[] = [];
  return {
    opened,
    open: (_room, options) => {
      void options;
      const stream = new FakeStream();
      opened.push(stream);
      return stream;
    },
  };
}

/** A scheduler that records instead of firing. */
function fakeScheduler(): RoomsScheduler & { pending: (() => void | Promise<unknown>)[] } {
  const pending: (() => void | Promise<unknown>)[] = [];
  return {
    pending,
    schedule: (callback) => {
      pending.push(callback);
      return pending.length;
    },
    cancel: () => undefined,
  };
}

describe('mergeMessages', () => {
  it('keeps one row per server id and the server order', () => {
    const merged = mergeMessages(
      [message(2), message(1)],
      [message(2), message(3), message(1)],
    );
    expect(merged.map((item) => item.sequence)).toEqual([1, 2, 3]);
    expect(new Set(merged.map((item) => item.id)).size).toBe(3);
  });

  it('keeps the server id and body when a live frame repeats a page message', () => {
    const fromPage = message(7, { id: 'server-7', body: 'from the page' });
    const live = { ...fromPage, body: 'from the live frame' };
    const merged = mergeMessages([fromPage], [live]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.id).toBe('server-7');
    expect(merged[0]?.body).toBe('from the live frame');
  });
});

describe('olderMessagesPage', () => {
  it('reads the page immediately before the oldest held message', async () => {
    const transport = fakeTransport(messages(1, 100));
    const page = await olderMessagesPage(transport, ROOM.id, 51, 50);
    expect(page.messages.map((item) => item.sequence)).toEqual(
      Array.from({ length: 50 }, (_, index) => index + 1),
    );
    expect(page.hasOlder).toBe(false);
    expect(transport.reads.every((read) => (read.after ?? 0) < 51)).toBe(true);
  });

  it('never overlaps the window it already holds', async () => {
    const transport = fakeTransport(messages(1, 100));
    const held = messages(51, 100);
    const page = await olderMessagesPage(transport, ROOM.id, tailCursor(held) ?? 0, 50);
    const merged = mergeMessages(held, page.messages);
    expect(merged).toHaveLength(100);
    expect(new Set(merged.map((item) => item.id)).size).toBe(100);
  });

  it('steps the cursor back across sequence gaps', async () => {
    const gapped = [message(5), message(60), message(61), message(62)];
    const transport = fakeTransport(gapped);
    const page = await olderMessagesPage(transport, ROOM.id, 60, 10);
    expect(page.messages.map((item) => item.sequence)).toEqual([5]);
    expect(page.hasOlder).toBe(false);
    expect(transport.reads.length).toBeGreaterThan(1);
  });
});

describe('catch-up labelling', () => {
  it('never says current while catching up', () => {
    const banner = currentnessBanner({
      catchingUp: true,
      current: true,
      presence: 'live',
    });
    expect(banner.status).toBe('catching-up');
    expect(banner.text).toBe(CATCH_UP_NOTICE);
    expect(isCurrent({ catchingUp: true, current: true, presence: 'live' })).toBe(false);
  });

  it('says current only after a confirmed fetch', () => {
    const banner = currentnessBanner({
      catchingUp: false,
      current: true,
      presence: 'live',
    });
    expect(banner.status).toBe('current');
    expect(banner.text).toBe(CURRENT_NOTICE);
    expect(isCurrent({ catchingUp: false, current: true, presence: 'live' })).toBe(true);
  });

  it('labels a dropped socket as reconnecting, not current', () => {
    const banner = currentnessBanner({
      catchingUp: true,
      current: false,
      presence: 'reconnecting',
    });
    expect(banner.status).toBe('reconnecting');
    expect(banner.text).toBe(RECONNECT_NOTICE);
  });
});

describe('author descriptors', () => {
  it('distinguishes a person from an agent session by word, glyph and label', () => {
    const person = describeAuthor({ kind: 'user', id: 'user-ada' });
    const agent = describeAuthor({ kind: 'session', id: 'session-7' });
    expect(person.kind).toBe('user');
    expect(agent.kind).toBe('session');
    expect(person.kindLabel).not.toBe(agent.kindLabel);
    expect(person.glyph).not.toBe(agent.glyph);
    expect(person.label).not.toBe(agent.label);
    expect(person.glyph.length).toBeGreaterThan(0);
    expect(agent.glyph.length).toBeGreaterThan(0);
  });

  it('keeps a deleted author readable as unknown', () => {
    expect(describeAuthor(null).kindLabel).toBe('Unknown author');
    expect(describeAuthor(undefined).id).toBeNull();
  });
});

describe('presence and room roles', () => {
  it('describes watchers without claiming messages depend on them', () => {
    expect(presenceSentence([])).toContain('No one else');
    expect(presenceSentence(['user-ada'])).toContain('1 watcher');
    expect(presenceSentence(['user-ada', 'user-grace'])).toContain('2 watchers');
  });

  it('withdraws the composer from a viewer and offers it to a member', () => {
    const members = [member('user-ada', 'user', 'viewer')];
    expect(roomRoleFor(members, 'user-ada')).toBe('viewer');
    expect(canPostInRoom(members, 'user-ada')).toBe(false);
    expect(canPostInRoom(members, 'user-grace')).toBe(true);
    expect(canPostInRoom([member('user-ada', 'user', 'member')], 'user-ada')).toBe(true);
  });

  it('asks for older messages only near the top', () => {
    expect(shouldLoadOlder(0, true, false)).toBe(true);
    expect(shouldLoadOlder(200, true, false)).toBe(false);
    expect(shouldLoadOlder(0, false, false)).toBe(false);
    expect(shouldLoadOlder(0, true, true)).toBe(false);
  });
});

describe('RoomsController', () => {
  /** A controller over one room, with a cache and no stream by default. */
  function controllerWith(
    transport: RoomsTransport,
    options: {
      openStream?: OpenRoomStream | null;
      cache?: RoomCache;
      scheduler?: RoomsScheduler;
      initial?: RoomsViewState;
      onChange?: (state: RoomsViewState) => void;
    } = {},
  ): RoomsController {
    return new RoomsController({
      transport,
      openStream: options.openStream ?? null,
      cache: options.cache,
      scheduler: options.scheduler,
      initial: options.initial,
      onChange: options.onChange,
      now: () => Date.parse('2026-10-29T06:00:00.000Z'),
    });
  }

  it('renders cached history as catching up until the fetch confirms currentness', async () => {
    const transport = fakeTransport(messages(1, 20));
    const cache: RoomCache = {
      read: () => messages(1, 20),
      write: () => undefined,
    };
    const controller = controllerWith(transport, { cache });
    await controller.loadRooms();
    const pending = controller.openRoom(ROOM.id);
    // The very first state is the cache plus a catch-up label.
    const opening = controller.snapshot();
    expect(opening.catchingUp).toBe(true);
    expect(opening.current).toBe(false);
    expect(opening.messages).toHaveLength(20);
    expect(currentnessBanner(opening).status).toBe('catching-up');

    const state = await pending;
    expect(state.current).toBe(true);
    expect(state.catchingUp).toBe(false);
    expect(state.messages).toHaveLength(20);
    expect(currentnessBanner(state).status).toBe('current');
  });

  it('notifies the page with the cached state before the fetch resolves', async () => {
    const base = fakeTransport(messages(1, 20));
    let releaseHistory: ((page: { messages: MessageView[]; next: number | null }) => void) | null =
      null;
    const transport: RoomsTransport = {
      ...base,
      history: (_room, query) => {
        void query;
        return new Promise((resolve) => {
          releaseHistory = resolve;
        });
      },
    };
    const cache: RoomCache = {
      read: () => messages(1, 20),
      write: () => undefined,
    };
    const seen: RoomsViewState[] = [];
    const controller = controllerWith(transport, {
      cache,
      onChange: (state) => seen.push(state),
    });
    await controller.loadRooms();
    const pending = controller.openRoom(ROOM.id);

    const cachedRender = seen.find(
      (state) => state.catchingUp && state.messages.length === 20 && state.historyPhase === 'ready',
    );
    expect(cachedRender).toBeDefined();
    expect(currentnessBanner(cachedRender as RoomsViewState).status).toBe('catching-up');

    const release = releaseHistory as unknown as (
      page: { messages: MessageView[]; next: number | null },
    ) => void;
    release({ messages: [], next: null });
    const state = await pending;
    expect(state.catchingUp).toBe(false);
    expect(state.current).toBe(true);
  });

  it('appends a live message under its server id and drops a duplicate', async () => {
    const transport = fakeTransport(messages(1, 10));
    const streams = fakeStreams();
    const seen: RoomsViewState[] = [];
    const controller = controllerWith(transport, {
      openStream: streams.open,
      onChange: (state) => seen.push(state),
    });
    await controller.loadRooms();
    await controller.openRoom(ROOM.id);
    const stream = streams.opened[0];
    expect(stream).toBeDefined();
    stream?.emit({
      type: 'chat.history',
      catchUp: true,
      messages: messages(1, 10),
    });
    expect(controller.snapshot().catchingUp).toBe(true);
    stream?.emit({ type: 'chat.current', catchUp: false, cursor: 10 });
    expect(controller.snapshot().current).toBe(true);

    const live = message(11, { id: 'server-eleven', body: 'live message' });
    stream?.emit({ type: 'chat.message', catchUp: false, message: live });
    stream?.emit({ type: 'chat.message', catchUp: false, message: live });
    const state = controller.snapshot();
    expect(state.messages).toHaveLength(11);
    expect(state.messages[10]?.id).toBe('server-eleven');
    expect(state.messages[10]?.body).toBe('live message');
    // The page receives the live frame, not only the state after an await.
    expect(
      seen.some((snapshot) =>
        snapshot.messages.some((item) => item.id === 'server-eleven'),
      ),
    ).toBe(true);
  });

  it('loads older pages through the cursor without duplication', async () => {
    const transport = fakeTransport(messages(1, 100));
    const controller = controllerWith(transport, {
      initial: stateWith({
        phase: 'ready',
        rooms: [ROOM],
        selectedRoomId: ROOM.id,
        room: ROOM,
        messages: messages(51, 100),
        hasOlder: true,
        current: true,
        catchingUp: false,
      }),
    });
    const state = await controller.loadOlder();
    expect(state.messages).toHaveLength(100);
    expect(new Set(state.messages.map((item) => item.id)).size).toBe(100);
    expect(state.messages[0]?.sequence).toBe(1);
  });

  it('drops back to catch-up when the socket closes and stays there until chat.current', async () => {
    const transport = fakeTransport(messages(1, 10));
    const streams = fakeStreams();
    const scheduler = fakeScheduler();
    const controller = controllerWith(transport, { openStream: streams.open, scheduler });
    await controller.loadRooms();
    await controller.openRoom(ROOM.id);
    const first = streams.opened[0];
    first?.emit({ type: 'chat.current', catchUp: false, cursor: 10 });
    expect(controller.snapshot().current).toBe(true);

    first?.drop();
    const dropped = controller.snapshot();
    expect(dropped.catchingUp).toBe(true);
    expect(dropped.current).toBe(false);
    expect(dropped.presence).toBe('reconnecting');
    expect(currentnessBanner(dropped).status).toBe('reconnecting');
    expect(scheduler.pending).toHaveLength(1);

    controller.reconnect();
    const reconnecting = controller.snapshot();
    expect(reconnecting.catchingUp).toBe(true);
    expect(reconnecting.current).toBe(false);
    const second = streams.opened[1];
    second?.emit({ type: 'chat.history', catchUp: true, messages: messages(9, 10) });
    expect(controller.snapshot().catchingUp).toBe(true);
    second?.emit({ type: 'chat.current', catchUp: false, cursor: 10 });
    expect(controller.snapshot().current).toBe(true);
    expect(currentnessBanner(controller.snapshot()).status).toBe('current');
  });

  it('treats presence changes as presence, never as message deletion', async () => {
    const transport = fakeTransport(messages(1, 5));
    const streams = fakeStreams();
    const controller = controllerWith(transport, { openStream: streams.open });
    await controller.loadRooms();
    await controller.openRoom(ROOM.id);
    const stream = streams.opened[0];
    stream?.emit({ type: 'chat.presence', catchUp: false, watchers: ['user-ada'] });
    expect(controller.snapshot().watchers).toEqual(['user-ada']);
    expect(controller.snapshot().messages).toHaveLength(5);
    stream?.emit({ type: 'chat.presence', catchUp: false, watchers: [] });
    expect(controller.snapshot().watchers).toEqual([]);
    expect(controller.snapshot().messages).toHaveLength(5);
  });

  it('posts a message and adopts the server copy', async () => {
    const transport = fakeTransport(messages(1, 3));
    const controller = controllerWith(transport);
    await controller.loadRooms();
    await controller.openRoom(ROOM.id);
    const state = await controller.send('hello from the page');
    expect(transport.sent).toEqual(['hello from the page']);
    expect(state.sendPhase).toBe('sent');
    expect(state.messages[state.messages.length - 1]?.body).toBe('hello from the page');
  });
});

describe('RoomsPage', () => {
  function roomsState(overrides: Partial<RoomsViewState> = {}): RoomsViewState {
    return stateWith({
      phase: 'ready',
      rooms: [ROOM, OTHER_ROOM],
      selectedRoomId: ROOM.id,
      room: ROOM,
      members: [member('user-ada', 'user', 'owner'), member('session-7', 'session')],
      membersPhase: 'ready',
      messages: [message(1), message(2, { author: { kind: 'session', id: 'session-7' } })],
      historyPhase: 'ready',
      catchingUp: true,
      current: false,
      presence: 'live',
      ...overrides,
    });
  }

  it('labels cached history as catching up', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState(),
    });
    expect(html).toContain('data-catch-up-status="catching-up"');
    expect(text).toContain('Catching up');
    expect(text).not.toContain('Current — every message');
    expect(text).toContain('Message 1');
    expect(text).toContain('builds');
  });

  it('says current once the state confirms it', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState({ catchingUp: false, current: true }),
    });
    expect(html).toContain('data-catch-up-status="current"');
    expect(text).toContain('Current — every message');
  });

  it('renders a person and an agent session with different words and glyphs', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState(),
    });
    expect(text).toContain('Person');
    expect(text).toContain('Agent session');
    expect(html).toContain('data-author-kind="user"');
    expect(html).toContain('data-author-kind="session"');
    expect(html).toContain('data-author-glyph="●"');
    expect(html).toContain('data-author-glyph="◆"');
    // The words carry the meaning; colour is never the only difference.
    expect(html).not.toBe(
      html.replace('data-author-kind="session"', 'data-author-kind="user"'),
    );
  });

  it('keeps the server id on every message row', () => {
    const { html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState(),
    });
    expect(html).toContain('data-message-id="message-1"');
    expect(html).toContain('data-message-sequence="2"');
  });

  it('offers older history only when there may be some', () => {
    const withOlder = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState({ hasOlder: true }),
    });
    expect(withOlder.text).toContain('Load older messages');
    expect(withOlder.html).toContain('data-rooms-action="older"');

    const atStart = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState({ hasOlder: false }),
    });
    expect(atStart.text).toContain('beginning of the room');
    expect(atStart.html).not.toContain('data-rooms-action="older"');
  });

  it('withdraws the composer from a viewer with the reason', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      viewerId: 'user-ada',
      initial: roomsState({ members: [member('user-ada', 'user', 'viewer')] }),
    });
    expect(text).toContain(VIEWER_CANNOT_POST);
    expect(text).not.toContain('Write a message to the room');
    expect(html).toContain('data-composer-denial="viewer"');
  });

  it('shows presence without hiding the history', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState({ watchers: ['user-ada'] }),
    });
    expect(text).toContain('1 watcher: user-ada');
    expect(text).toContain('Message 1');
    expect(html).toContain('data-presence-watchers="1"');
  });

  it('renders the unconfigured state without a room list', () => {
    const { text, html } = renderComponent(RoomsPage, {
      pool: { baseUrl: null },
      initial: emptyRoomsState(),
    });
    expect(text).toContain('No pool is configured');
    expect(html).toContain('data-rooms-state="unconfigured"');
  });

  it('asks for a room when none is selected', () => {
    const { text } = renderComponent(RoomsPage, {
      pool: POOL,
      token: null,
      initial: roomsState({ selectedRoomId: null, room: null, messages: [] }),
    });
    expect(text).toContain('Choose a room');
  });
});
