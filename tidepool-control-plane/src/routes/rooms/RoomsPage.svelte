<script lang="ts">
  /**
   * The chatrooms page.
   *
   * The left column is the member's rooms; the right column is one room, its
   * currentness banner, its history, its composer and its members. The history
   * merges by the pool's message id and orders by its sequence, so a page, a
   * live frame and a reconnect replay show each message once. Scrolling to the
   * top (or pressing the older button) loads the previous page through the
   * cursor. A person and an agent session are labelled and glyphed differently,
   * and the banner says "catching up" until the pool confirms the room is
   * current — including while a dropped socket is reconnecting.
   *
   * The page renders from a `RoomsViewState`, loads itself from the shell's pool
   * and token when it has no supplied state, and accepts a transport and stream
   * factory so the component tests and the two-browser end-to-end can run
   * without a live pool or socket.
   *
   * The create panel is one form: a project, a name and a visibility, all on the
   * shared field component. Creating reloads the pool's list rather than
   * appending a guess, and each row shows the room's name and its last activity —
   * the newest message's time, or "No messages yet" for an empty room.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import FormField from '../../components/FormField.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import CatchUpBanner from './CatchUpBanner.svelte';
  import MessageComposer from './MessageComposer.svelte';
  import MessageList from './MessageList.svelte';
  import RoomMembers from './RoomMembers.svelte';
  import { isPoolConfigured, poolClient, type PoolConfig } from '$lib/api/pool';
  import type { RoomView } from '@clawffice/tidepool-sdk';
  import {
    DEFAULT_ROOM_VISIBILITY,
    ROOM_VISIBILITIES,
    ROOM_CREATING,
    RoomsController,
    canLoadRooms,
    canPostInRoom,
    defaultProjectId,
    emptyRoomsState,
    lastActivityView,
    presenceSentence,
    roomStreamFactory,
    roomsTransport,
    validateRoomInput,
    visibilityDescription,
    visibilityLabel,
    type OpenRoomStream,
    type RoomsTransport,
    type RoomsViewState,
  } from '$lib/rooms';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in member's id, for the member list and the composer gate. */
    viewerId?: string | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: RoomsViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: RoomsTransport | null;
    /** Stream factory override, for tests. */
    openStream?: OpenRoomStream | null;
    /** The instant relative activity is measured against; defaults to now. */
    now?: number | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    viewerId = null,
    initial = emptyRoomsState(),
    transport = null,
    openStream = null,
    now = null,
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let view = $state<RoomsViewState>(initial);

  /** The instant activity ages are measured against. */
  function currentInstant(): number {
    return now ?? Date.now();
  }

  /** The create form's own state; it never lives in the shared view state. */
  // svelte-ignore state_referenced_locally
  let project = $state(defaultProjectId(initial.projects));
  let name = $state('');
  let visibility = $state<RoomView['visibility']>(DEFAULT_ROOM_VISIBILITY);
  /** The one validation failure, tied to the field it names. */
  let failure = $state<{ field: 'project' | 'name'; message: string } | null>(null);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: RoomsController | null = (() => {
    const onChange = (next: RoomsViewState): void => {
      view = next;
    };
    if (transport) return new RoomsController({ transport, openStream, onChange });
    if (!canLoadRooms(pool, token)) return null;
    try {
      const client = poolClient(pool, { token });
      return new RoomsController({
        transport: roomsTransport(client),
        openStream: openStream ?? roomStreamFactory(client),
        onChange,
      });
    } catch {
      return null;
    }
  })();

  // svelte-ignore state_referenced_locally
  const configured = transport !== null || isPoolConfigured(pool);
  const canPost = $derived(canPostInRoom(view.members, viewerId));
  const selected = $derived(view.selectedRoomId !== null);

  /** Adopt a controller result. */
  function adopt(next: RoomsViewState): void {
    view = next;
  }

  /** Load the member's rooms. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.loadRooms());
  }

  /** Create a room from the form, then adopt the reloaded list. */
  async function submitCreate(): Promise<void> {
    if (!controller || view.createPhase === 'creating') return;
    failure = null;
    const validation = validateRoomInput({ project, name, visibility });
    if (!validation.ok) {
      failure = { field: validation.field, message: validation.message };
      return;
    }
    const next = await controller.createRoom(validation.input);
    adopt(next);
    if (next.createPhase === 'created') {
      name = '';
      if (next.projects.length > 1) project = '';
    }
  }

  /** The form's submit handler: never leave the page. */
  async function create(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    await submitCreate();
  }

  /** Retry a failed create with the same form values. */
  async function createByForm(): Promise<void> {
    await submitCreate();
  }

  /** Retry the project list. */
  async function loadProjectsAgain(): Promise<void> {
    if (!controller) return;
    adopt(await controller.loadProjects());
  }

  /** Open one room. */
  async function open(roomId: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.openRoom(roomId));
  }

  /** Read one older page. */
  async function loadOlder(): Promise<void> {
    if (!controller) return;
    adopt(await controller.loadOlder());
  }

  /** Send one message. */
  async function send(body: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.send(body));
  }

  /** Reopen the live stream after a drop. */
  function reconnect(): void {
    if (!controller) return;
    adopt(controller.reconnect());
  }

  /** Close the selected room. */
  function close(): void {
    if (!controller) return;
    adopt(controller.closeRoom());
  }

  $effect(() => {
    // A page with a pool and no supplied state loads the room list once.
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void controller.loadRooms().then(adopt);
      void controller.loadProjects().then(adopt);
    }
  });

  $effect(() => {
    // Once the projects arrive, a lone project is the obvious default.
    if (project === '' && view.projects.length === 1) {
      project = defaultProjectId(view.projects);
    }
  });

  $effect(() => () => controller?.dispose());
</script>

<section class="tp-rooms tp-page" aria-labelledby="tp-rooms-title" data-rooms-page="true">
  <h1 class="tp-page__title" id="tp-rooms-title">Rooms</h1>
  <p class="tp-page__lede">
    The rooms you belong to, who is in them and who is watching, the history
    read through the pool’s cursor, and messages as they arrive. Cached history
    is labelled as catching up until the pool confirms the room is current, and
    a person and an agent session are labelled differently.
  </p>

  {#if !configured}
    <p class="tp-page__state" data-rooms-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there are no rooms to read. Point the control
        plane at a pool and sign in.
      </span>
    </p>
  {:else}
    <section class="tp-rooms__create-panel" aria-labelledby="tp-rooms-create-title">
      <h2 class="tp-rooms__section-title" id="tp-rooms-create-title">Create a room</h2>
      <p class="tp-rooms__create-lede">
        A room belongs to one project. You are added as its owner, so it appears
        in the list right away.
      </p>

      {#if view.projectsProblem}
        <ProblemDetails problem={view.projectsProblem} onRetry={() => void loadProjectsAgain()} />
      {/if}

      {#if view.createProblem}
        <ProblemDetails problem={view.createProblem} onRetry={() => void createByForm()} />
      {/if}

      {#if view.createNotice}
        <p class="tp-rooms__notice" role="status" data-room-created={view.createdRoomId ?? ''}>
          <StatusBadge kind="success" />
          <span>{view.createNotice}</span>
        </p>
      {/if}

      <form class="tp-rooms__create-form" onsubmit={create} data-form="room_create">
        <FormField
          form="room_create"
          field="project"
          label="Project"
          required
          help={view.projects.length === 0
            ? 'No projects exist yet, so a room cannot belong to one.'
            : 'The project the room and its messages belong to.'}
          error={failure?.field === 'project' ? failure.message : null}
        >
          {#snippet children(id, described)}
            <select
              class="tp-field__control"
              {id}
              name="room_project"
              aria-invalid={failure?.field === 'project'}
              aria-describedby={described}
              required
              bind:value={project}
            >
              <option value="" disabled>
                {view.projects.length ? 'Choose a project' : 'No projects yet'}
              </option>
              {#each view.projects as candidate (candidate.id)}
                <option value={candidate.id}>{candidate.name} ({candidate.slug})</option>
              {/each}
            </select>
          {/snippet}
        </FormField>

        <FormField
          form="room_create"
          field="name"
          label="Room name"
          required
          help="Unique inside the project, at most 120 characters, e.g. builds."
          error={failure?.field === 'name' ? failure.message : null}
        >
          {#snippet children(id, described)}
            <input
              class="tp-field__control"
              {id}
              type="text"
              name="room_name"
              autocomplete="off"
              spellcheck="false"
              required
              aria-invalid={failure?.field === 'name'}
              aria-describedby={described}
              bind:value={name}
            />
          {/snippet}
        </FormField>

        <FormField
          form="room_create"
          field="visibility"
          label="Visibility"
          help={visibilityDescription(visibility)}
        >
          {#snippet children(id, described)}
            <select
              class="tp-field__control"
              {id}
              name="room_visibility"
              aria-describedby={described}
              bind:value={visibility}
            >
              {#each ROOM_VISIBILITIES as option (option)}
                <option value={option}>{visibilityLabel(option)}</option>
              {/each}
            </select>
          {/snippet}
        </FormField>

        <div class="tp-field__actions">
          <button
            class="tp-button"
            type="submit"
            data-rooms-action="create"
            disabled={view.createPhase === 'creating' || view.busy}
          >
            {view.createPhase === 'creating' ? ROOM_CREATING : 'Create the room'}
          </button>
        </div>
      </form>
    </section>

    <div class="tp-rooms__layout">
      <section class="tp-rooms__list-panel" aria-labelledby="tp-rooms-list-title">
        <div class="tp-rooms__section-header">
          <h2 class="tp-rooms__section-title" id="tp-rooms-list-title">Your rooms</h2>
          <button
            class="tp-button"
            type="button"
            data-rooms-action="refresh"
            disabled={view.busy}
            onclick={() => void refresh()}
          >
            Refresh
          </button>
        </div>

        {#if view.roomsProblem}
          <ProblemDetails problem={view.roomsProblem} onRetry={() => void refresh()} />
        {/if}

        {#if view.phase === 'loading' && view.rooms.length === 0}
          <p class="tp-page__state" data-rooms-state="loading">
            <StatusBadge kind="pending" />
            <span>Reading your rooms…</span>
          </p>
        {:else if view.rooms.length === 0}
          <p class="tp-page__state" data-rooms-state="empty">
            <StatusBadge kind="neutral" />
            <span>You are not a member of any room in this pool yet.</span>
          </p>
        {:else}
          <ul class="tp-rooms__rooms" data-room-list="true">
            {#each view.rooms as room (room.id)}
              <li class="tp-rooms__room-item">
                <button
                  class="tp-rooms__room"
                  type="button"
                  data-room-id={room.id}
                  data-room-selected={view.selectedRoomId === room.id ? 'true' : 'false'}
                  aria-current={view.selectedRoomId === room.id ? 'true' : undefined}
                  onclick={() => void open(room.id)}
                >
                  <span class="tp-rooms__room-name">{room.name}</span>
                  <span
                    class="tp-rooms__room-meta"
                    data-room-last-activity={lastActivityView(room.last_activity_at, currentInstant())
                      .at ?? 'none'}
                  >
                    {visibilityLabel(room.visibility)} · {lastActivityView(
                      room.last_activity_at,
                      currentInstant(),
                    ).text}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section class="tp-rooms__room-panel" aria-labelledby="tp-rooms-room-title">
        {#if !selected || view.room === null}
          <p class="tp-page__state" data-rooms-state="no-room">
            <StatusBadge kind="neutral" />
            <span>Choose a room to read its history and watch it live.</span>
          </p>
        {:else}
          <div class="tp-rooms__room-header">
            <div>
              <h2 class="tp-rooms__section-title" id="tp-rooms-room-title" data-room-title="true">
                {view.room.name}
              </h2>
              <p class="tp-rooms__room-facts" data-room-visibility={view.room.visibility}>
                {visibilityLabel(view.room.visibility)} room · created {view.room.created_at} · {lastActivityView(
                  view.room.last_activity_at,
                  currentInstant(),
                ).text}
              </p>
            </div>
            <div class="tp-rooms__room-actions">
              {#if view.reconnectNeeded}
                <button
                  class="tp-button"
                  type="button"
                  data-rooms-action="reconnect"
                  onclick={reconnect}
                >
                  Reconnect
                </button>
              {/if}
              <button class="tp-button" type="button" data-rooms-action="close" onclick={close}>
                Close room
              </button>
            </div>
          </div>

          <CatchUpBanner state={view} />

          {#if view.historyProblem}
            <ProblemDetails
              problem={view.historyProblem}
              onRetry={() => void open(view.room?.id ?? '')}
            />
          {/if}
          {#if view.olderProblem}
            <ProblemDetails problem={view.olderProblem} onRetry={() => void loadOlder()} />
          {/if}

          <p class="tp-rooms__presence" data-room-presence={view.presence}>
            <StatusBadge kind={view.presence === 'live' ? 'success' : 'pending'} />
            <span>{presenceSentence(view.watchers)}</span>
          </p>

          {#if view.historyPhase === 'loading' && view.messages.length === 0}
            <p class="tp-page__state" data-rooms-state="history-loading">
              <StatusBadge kind="pending" />
              <span>Reading the room history…</span>
            </p>
          {:else}
            <MessageList
              messages={view.messages}
              hasOlder={view.hasOlder}
              loadingOlder={view.loadingOlder}
              catchingUp={view.catchingUp}
              onLoadOlder={() => void loadOlder()}
            />
          {/if}

          <MessageComposer
            canPost={canPost}
            sendPhase={view.sendPhase}
            problem={view.sendProblem}
            onSend={(body) => void send(body)}
          />

          <RoomMembers
            members={view.members}
            watchers={view.watchers}
            viewerId={viewerId}
            loading={view.membersPhase === 'loading'}
          />

          {#if view.membersProblem}
            <ProblemDetails problem={view.membersProblem} />
          {/if}
        {/if}
      </section>
    </div>
  {/if}
</section>

<style>
  .tp-rooms {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-rooms__create-panel {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-rooms__create-lede {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-rooms__create-form {
    display: grid;
    gap: var(--tp-space-2);
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  }

  .tp-rooms__create-form .tp-field__actions {
    grid-column: 1 / -1;
  }

  .tp-rooms__notice {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--tp-space-2);
    margin: 0;
    color: var(--tp-text-strong);
    font-size: var(--tp-font-caption);
  }

  .tp-rooms__layout {
    display: grid;
    gap: var(--tp-space-3);
    grid-template-columns: minmax(14rem, 0.7fr) minmax(20rem, 1.6fr);
    align-items: start;
  }

  @media (max-width: 60rem) {
    .tp-rooms__layout {
      grid-template-columns: 1fr;
    }
  }

  .tp-rooms__list-panel,
  .tp-rooms__room-panel {
    display: grid;
    gap: var(--tp-space-2);
    align-content: start;
  }

  .tp-rooms__section-header,
  .tp-rooms__room-header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-rooms__section-title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-rooms__rooms {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-rooms__room {
    display: grid;
    gap: 2px;
    width: 100%;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface);
    color: var(--tp-text);
    text-align: left;
    font: inherit;
    cursor: pointer;
  }

  .tp-rooms__room[data-room-selected='true'] {
    border-color: var(--tp-primary);
    background: var(--tp-surface-raised, var(--tp-surface));
  }

  .tp-rooms__room-name {
    font-weight: 600;
    color: var(--tp-text-strong);
  }

  .tp-rooms__room-meta,
  .tp-rooms__room-facts,
  .tp-rooms__presence {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-rooms__presence {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--tp-space-2);
  }

  .tp-rooms__room-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }
</style>
