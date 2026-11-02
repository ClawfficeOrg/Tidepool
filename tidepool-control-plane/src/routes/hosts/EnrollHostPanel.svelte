<script lang="ts">
  /**
   * Enrol a host, then show its one-time token and the command to run.
   *
   * The token is single use and the pool keeps only a hash, so the result
   * replaces the form and says plainly that this is the only time it can be
   * shown. The command is the exact shell a person runs on the host machine,
   * built from the pool URL and the token; it is not a summary of one.
   */
  import HostStatusBadge from './HostStatusBadge.svelte';
  import type { EnrollmentResult } from '$lib/hosts';
  import { validateEnrollInput } from '$lib/hosts';

  interface Props {
    /** The pool the command will point at. */
    poolUrl?: string | null;
    /** True while the enrolment request is in flight. */
    busy?: boolean;
    /** The token and command, once the pool has answered. */
    enrollment?: EnrollmentResult | null;
    /** Called with a valid form. */
    onSubmit?: (input: { hostname: string; display_name: string | null }) => void;
    /** Called when the person has copied the token and command. */
    onDismiss?: () => void;
  }

  let { poolUrl = null, busy = false, enrollment = null, onSubmit, onDismiss }: Props = $props();

  let hostname = $state('');
  let displayName = $state('');
  let localError = $state<string | null>(null);
  let copied = $state(false);

  /** Validate here first, so an empty form never becomes a round trip. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    if (busy) return;
    localError = null;
    const validation = validateEnrollInput({ hostname, display_name: displayName });
    if (!validation.ok) {
      localError = validation.message;
      return;
    }
    onSubmit?.(validation.input);
  }

  /** Copy the command, when the browser allows it. */
  async function copyCommand(): Promise<void> {
    if (!enrollment) return;
    try {
      await navigator.clipboard.writeText(enrollment.command);
      copied = true;
    } catch {
      copied = false;
    }
  }
</script>

<section class="tp-enrol" aria-labelledby="tp-enrol-title" data-enrol-panel="true">
  <h2 class="tp-enrol__title" id="tp-enrol-title">Enrol a host</h2>

  {#if enrollment}
    <div class="tp-enrol__result" data-enrol-result="true">
      <p class="tp-enrol__state">
        <HostStatusBadge status={enrollment.host.status.status} />
        <span>
          <strong>{enrollment.host.slug}</strong> is registered and waiting for its first
          connection.
        </span>
      </p>

      <div class="tp-enrol__token">
        <span class="tp-enrol__label">One-time enrolment token</span>
        <code class="tp-enrol__secret" data-enrollment-token>{enrollment.token}</code>
      </div>

      <p class="tp-enrol__warning" role="note">{enrollment.warning}</p>
      <p class="tp-enrol__expiry">
        Expires at <time datetime={enrollment.expiresAt}>{enrollment.expiresAt}</time>.
      </p>

      <div class="tp-enrol__command">
        <span class="tp-enrol__label">Run this on the host machine</span>
        <pre data-enrollment-command><code>{enrollment.command}</code></pre>
      </div>

      <p class="tp-enrol__note">
        The token is consumed by that run. After it enrols, start the agent with
        <code>HOST_ID={enrollment.host.id}</code> and the credential file it created,
        without the token. Grant execution below when you want the pool to dispatch
        runs to it.
      </p>

      <div class="tp-enrol__buttons">
        <button class="tp-button tp-button--quiet" type="button" onclick={copyCommand}>
          {copied ? 'Copied' : 'Copy command'}
        </button>
        <button class="tp-button" type="button" onclick={() => onDismiss?.()}>
          I have copied the token and command
        </button>
      </div>
    </div>
  {:else}
    <p class="tp-enrol__lede">
      Register a machine and the pool mints a single-use token for it. The command
      below the token is what you run on that machine
      {#if poolUrl}against <code>{poolUrl}</code>{/if}; enrolment alone does not let
      it run agents.
    </p>

    {#if localError}
      <p class="tp-enrol__error" role="alert">{localError}</p>
    {/if}

    <form class="tp-enrol__form" onsubmit={submit}>
      <label class="tp-enrol__field">
        <span class="tp-enrol__label">Hostname</span>
        <input
          class="tp-enrol__input"
          type="text"
          name="hostname"
          autocomplete="off"
          spellcheck="false"
          required
          bind:value={hostname}
        />
        <span class="tp-enrol__hint">
          The name the machine reports, e.g. <code>swaygent.local</code>. The pool
          derives the slug from it.
        </span>
      </label>

      <label class="tp-enrol__field">
        <span class="tp-enrol__label">Display name (optional)</span>
        <input
          class="tp-enrol__input"
          type="text"
          name="display_name"
          autocomplete="off"
          bind:value={displayName}
        />
        <span class="tp-enrol__hint">What the hosts list calls this machine.</span>
      </label>

      <button class="tp-button" type="submit" disabled={busy}>
        {busy ? 'Minting the token…' : 'Enrol and mint the token'}
      </button>
    </form>
  {/if}
</section>

<style>
  .tp-enrol {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-enrol__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-enrol__lede,
  .tp-enrol__note,
  .tp-enrol__warning,
  .tp-enrol__expiry {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
  }

  .tp-enrol__warning {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-warning);
    padding-left: var(--tp-space-2);
  }

  .tp-enrol__form,
  .tp-enrol__result,
  .tp-enrol__token,
  .tp-enrol__command {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-enrol__field {
    display: grid;
    gap: 2px;
  }

  .tp-enrol__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-enrol__hint {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-enrol__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-enrol__state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-2);
    margin: 0;
  }

  .tp-enrol__secret {
    overflow-wrap: anywhere;
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    color: var(--tp-text-strong);
    background: var(--tp-terminal-background);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    padding: var(--tp-space-1) var(--tp-space-2);
  }

  .tp-enrol__command pre {
    margin: 0;
    padding: var(--tp-space-2);
    overflow-x: auto;
    background: var(--tp-terminal-background);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    font-size: var(--tp-font-code);
  }

  .tp-enrol__error {
    margin: 0;
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-danger);
    padding-left: var(--tp-space-2);
  }

  .tp-enrol__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  /* A secondary action: the same hit target, without the primary weight. */
  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
