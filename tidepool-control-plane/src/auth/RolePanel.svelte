<script lang="ts">
  /**
   * What the signed-in role may do.
   *
   * Allowed controls are listed as actions the pages will offer; controls the
   * role cannot use are never rendered as controls. They appear in a closed
   * disclosure with the reason, so a viewer understands the absence instead of
   * meeting a button that fails on submit.
   */
  import { availableControls, hiddenControls, roleDescription, roleLabel } from './roles';
  import type { Role } from './types';

  interface Props {
    /** The signed-in operator's role. */
    role: Role;
    /** The operator's display name, when the shell has it. */
    name?: string;
    /** The organization name, when known. */
    organization?: string | null;
  }

  let { role, name = '', organization = null }: Props = $props();

  const available = $derived(availableControls(role));
  const hidden = $derived(hiddenControls(role));
</script>

<section class="tp-role" aria-labelledby="tp-role-title" data-role={role}>
  <h2 class="tp-role__title" id="tp-role-title">What {roleLabel(role).toLowerCase()} can do</h2>
  <p class="tp-role__lede">
    {#if name}<strong>{name}</strong>{/if}
    {#if organization}· {organization}{/if}
    · {roleDescription(role)}
  </p>

  <ul class="tp-role__list" aria-label="Available actions">
    {#each available as control (control.id)}
      <li class="tp-role__item" data-control={control.id}>
        <span class="tp-role__label">{control.label}</span>
        <span class="tp-role__description">{control.description}</span>
      </li>
    {/each}
  </ul>

  {#if hidden.length > 0}
    <details class="tp-role__hidden">
      <summary>{hidden.length} actions are hidden for this role</summary>
      <ul class="tp-role__list" aria-label="Hidden actions and why">
        {#each hidden as entry (entry.control.id)}
          <li class="tp-role__item" data-hidden-control={entry.control.id}>
            <span class="tp-role__description">{entry.reason}</span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
</section>

<style>
  .tp-role {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-4);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-role__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-role__lede {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
  }

  .tp-role__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-role__item {
    display: grid;
    gap: 2px;
    padding: var(--tp-space-1) var(--tp-space-2);
    border-left: 2px solid var(--tp-focus);
    background: var(--tp-terminal-background);
    border-radius: var(--tp-radius-control);
  }

  .tp-role__label {
    font-weight: 600;
    color: var(--tp-text-strong);
  }

  .tp-role__description {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-role__hidden summary {
    cursor: pointer;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-role__hidden .tp-role__item {
    border-left-color: var(--tp-border-strong);
    margin-top: var(--tp-space-1);
  }
</style>
