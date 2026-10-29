<script lang="ts">
  /**
   * The members of a room, and who is watching now.
   *
   * A member's principal kind is rendered with the same descriptor as a
   * message author, so an agent session is a diamond and the words "Agent
   * session" wherever it appears. Presence is separate from delivery: a watcher
   * disappearing never removes a message, it only changes this list.
   */
  import type { RoomMemberView } from '@clawffice/tidepool-sdk';
  import AuthorBadge from './AuthorBadge.svelte';
  import { describeMember, memberIsSession, memberRoleLabel, presenceSentence } from '$lib/rooms';

  interface Props {
    /** The room's members. */
    members: readonly RoomMemberView[];
    /** The principals the live stream reports watching now. */
    watchers?: readonly string[];
    /** The signed-in member's id, so their own row can be marked. */
    viewerId?: string | null;
    /** True while the member list is loading. */
    loading?: boolean;
  }

  let { members, watchers = [], viewerId = null, loading = false }: Props = $props();
  const watching = $derived(new Set(watchers));
  const sessionCount = $derived(members.filter(memberIsSession).length);
</script>

<section class="tp-members" aria-labelledby="tp-members-title" data-members="true">
  <h2 class="tp-members__title" id="tp-members-title">Members</h2>
  <p class="tp-members__summary" data-members-count={members.length}>
    {members.length} member{members.length === 1 ? '' : 's'}
    {#if sessionCount > 0}
      · {sessionCount} agent session{sessionCount === 1 ? '' : 's'}
    {/if}
  </p>
  <p class="tp-members__presence" data-presence-watchers={watchers.length}>
    {presenceSentence(watchers)}
  </p>

  {#if loading && members.length === 0}
    <p class="tp-members__state">Reading the member list…</p>
  {:else if members.length === 0}
    <p class="tp-members__state" data-members-empty="true">
      The pool reported no members for this room.
    </p>
  {:else}
    <ul class="tp-members__list">
      {#each members as member (member.id)}
        {@const descriptor = describeMember(member)}
        <li
          class="tp-members__item"
          data-member-id={member.id}
          data-member-kind={descriptor.kind}
          data-member-role={member.role}
        >
          <AuthorBadge author={member.principal} />
          <span class="tp-members__role">{memberRoleLabel(member.role)}</span>
          {#if viewerId !== null && member.principal.kind === 'user' && member.principal.id === viewerId}
            <span class="tp-members__you" data-member-you="true">You</span>
          {/if}
          <span
            class="tp-members__watch"
            data-member-watching={watching.has(member.principal.id) ? 'true' : 'false'}
          >
            {watching.has(member.principal.id) ? 'Watching now' : 'Not watching'}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .tp-members {
    display: grid;
    gap: var(--tp-space-1);
    align-content: start;
  }

  .tp-members__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-members__summary,
  .tp-members__presence,
  .tp-members__state {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-members__list {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-members__item {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--tp-space-2);
    padding: var(--tp-space-1) 0;
    border-bottom: 1px solid var(--tp-border);
  }

  .tp-members__role,
  .tp-members__watch {
    font-size: var(--tp-font-caption);
    color: var(--tp-text-muted);
  }

  .tp-members__you {
    font-size: var(--tp-font-caption);
    font-weight: 600;
    color: var(--tp-text-strong);
  }

  .tp-members__item[data-member-kind='session'] {
    border-left: 2px solid var(--tp-accent);
    padding-left: var(--tp-space-2);
  }
</style>
