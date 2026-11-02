<script lang="ts">
  /**
   * The settings a person owns: pool URL, palette, clock, density and about.
   *
   * Everything here is read from and written to the browser's storage, so the
   * change applies to the running app and survives a reload. The palette
   * switch is the shell's own control (`PaletteSwitcher`): the choice is made
   * in one place and every page renders it, rather than only this page.
   *
   * The about block reports the control plane's version and the hub contract
   * version, which is the pair a person needs when a pool is older than the
   * app talking to it.
   */
  import PaletteSwitcher from '../../components/PaletteSwitcher.svelte';
  import type { PaletteName } from '$lib/design/tokens';
  import {
    CONTROL_PLANE_VERSION,
    DENSITIES,
    HUB_CONTRACT_VERSION,
    TIME_FORMATS,
    TIME_ZONES,
    densityLabel,
    formatTime,
    timeFormatLabel,
    timeZoneLabel,
    type ControlPlanePreferences,
    type Density,
    type TimeFormat,
    type TimeZonePreference,
  } from './preferences';

  interface Props {
    /** The preferences currently in effect. */
    preferences: ControlPlanePreferences;
    /** The pool this build is currently pointed at, for the sentence of record. */
    effectivePoolUrl?: string | null;
    /** True when the page itself was served by the pool. */
    servedByPool?: boolean;
    /** Called when a new pool URL is submitted. */
    onSavePoolUrl?: (url: string) => void;
    /** Called with a palette choice; the shell remembers it. */
    onPaletteSelect?: (name: PaletteName) => void;
    /** Called with any other preference change. */
    onPreferenceChange?: (patch: Partial<ControlPlanePreferences>) => void;
    /** A rejected pool URL, already in words. */
    poolUrlProblem?: string | null;
    /** A saved pool URL notice. */
    poolUrlNotice?: string | null;
    /** An instant to preview the clock preference with. */
    previewInstant?: string;
  }

  let {
    preferences,
    effectivePoolUrl = null,
    servedByPool = false,
    onSavePoolUrl,
    onPaletteSelect,
    onPreferenceChange,
    poolUrlProblem = null,
    poolUrlNotice = null,
    previewInstant = '2026-10-29T14:05:09.000Z',
  }: Props = $props();

  // The field is seeded from the saved URL; the person drives it afterwards.
  // svelte-ignore state_referenced_locally
  let poolUrl = $state(preferences.poolUrl ?? '');
  let submitted = $state(false);

  /** Send the URL; the page validates and reports back in words. */
  function savePool(event: SubmitEvent): void {
    event.preventDefault();
    submitted = true;
    onSavePoolUrl?.(poolUrl);
  }
</script>

<section class="tp-prefs tp-page" aria-labelledby="tp-prefs-title" data-settings-preferences="true">
  <h2 class="tp-prefs__title" id="tp-prefs-title">This control plane</h2>

  <div class="tp-prefs__group" data-prefs-group="pool">
    <h3 class="tp-prefs__heading">Pool URL</h3>
    <p class="tp-prefs__lede">
      The pool this browser talks to. It is saved here and used immediately, so
      pointing at a different pool never needs a rebuild. Leave it empty to use
      the pool that served this page.
    </p>
    <form class="tp-prefs__form" onsubmit={savePool}>
      <label class="tp-prefs__field">
        <span class="tp-prefs__label">Pool URL</span>
        <input
          class="tp-prefs__input"
          type="url"
          name="pool-url"
          inputmode="url"
          autocomplete="off"
          spellcheck="false"
          placeholder="https://pool.example.com"
          bind:value={poolUrl}
          data-pool-url-input="true"
        />
        <span class="tp-prefs__hint">
          {#if effectivePoolUrl}
            In effect: <code data-effective-pool-url>{effectivePoolUrl}</code>
            {#if servedByPool}
              (served by the pool itself)
            {/if}
          {:else}
            No pool in effect yet. Enter an absolute URL to connect.
          {/if}
        </span>
      </label>
      <button class="tp-button" type="submit">Save pool URL</button>
    </form>
    {#if poolUrlProblem}
      <p class="tp-prefs__error" role="alert" data-pool-url-problem="true">
        {poolUrlProblem}
      </p>
    {:else if submitted && poolUrlNotice}
      <p class="tp-prefs__notice" role="status" data-pool-url-notice="true">
        {poolUrlNotice}
      </p>
    {/if}
  </div>

  <div class="tp-prefs__group" data-prefs-group="palette">
    <h3 class="tp-prefs__heading">Palette</h3>
    <p class="tp-prefs__lede">
      All five Clawffice palettes. Biolume is the default; the choice is
      remembered and every page renders it.
    </p>
    <PaletteSwitcher palette={preferences.palette} onSelect={onPaletteSelect} />
  </div>

  <div class="tp-prefs__group" data-prefs-group="time">
    <h3 class="tp-prefs__heading">Time</h3>
    <p class="tp-prefs__lede">
      How instants from the pool are written. The preview uses this moment:
      <code>{formatTime(previewInstant, preferences, { seconds: true, date: true })}</code>.
    </p>
    <div class="tp-prefs__choices">
      <fieldset class="tp-prefs__choice-set">
        <legend class="tp-prefs__label">Clock</legend>
        {#each TIME_FORMATS as option (option)}
          <label class="tp-prefs__choice">
            <input
              type="radio"
              name="time-format"
              value={option}
              checked={preferences.timeFormat === option}
              onchange={() => onPreferenceChange?.({ timeFormat: option as TimeFormat })}
              data-time-format-option={option}
            />
            <span>{timeFormatLabel(option)}</span>
          </label>
        {/each}
      </fieldset>

      <fieldset class="tp-prefs__choice-set">
        <legend class="tp-prefs__label">Time zone</legend>
        {#each TIME_ZONES as option (option)}
          <label class="tp-prefs__choice">
            <input
              type="radio"
              name="time-zone"
              value={option}
              checked={preferences.timeZone === option}
              onchange={() => onPreferenceChange?.({ timeZone: option as TimeZonePreference })}
              data-time-zone-option={option}
            />
            <span>{timeZoneLabel(option)}</span>
          </label>
        {/each}
      </fieldset>
    </div>
  </div>

  <div class="tp-prefs__group" data-prefs-group="density">
    <h3 class="tp-prefs__heading">Density</h3>
    <p class="tp-prefs__lede">
      Comfortable spacing, or compact when more rows matter than air. The choice
      applies to every page.
    </p>
    <fieldset class="tp-prefs__choice-set">
      <legend class="tp-prefs__label">Spacing</legend>
      {#each DENSITIES as option (option)}
        <label class="tp-prefs__choice">
          <input
            type="radio"
            name="density"
            value={option}
            checked={preferences.density === option}
            onchange={() => onPreferenceChange?.({ density: option as Density })}
            data-density-option={option}
          />
          <span>{densityLabel(option)}</span>
        </label>
      {/each}
    </fieldset>
  </div>

  <div class="tp-prefs__group" data-prefs-group="about">
    <h3 class="tp-prefs__heading">About</h3>
    <dl class="tp-prefs__about">
      <dt>Control plane</dt>
      <dd>
        <code data-control-plane-version>{CONTROL_PLANE_VERSION}</code>
      </dd>
      <dt>Hub contract</dt>
      <dd>
        <code data-hub-contract-version>{HUB_CONTRACT_VERSION}</code>
      </dd>
      <dt>Served by</dt>
      <dd data-served-by={servedByPool ? 'pool' : 'standalone'}>
        {servedByPool
          ? "This pool, from its own origin — one origin, no CORS."
          : 'A standalone deployment, pointed at the pool above.'}
      </dd>
    </dl>
    <p class="tp-prefs__hint">
      The standalone deployment and the pool-served copy are the same built app;
      only the origin it was opened from differs.
    </p>
  </div>
</section>

<style>
  .tp-prefs {
    display: grid;
    gap: var(--tp-space-4);
  }

  .tp-prefs__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-prefs__group {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-prefs__heading {
    margin: 0;
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-prefs__lede,
  .tp-prefs__hint {
    margin: 0;
    max-width: 72ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-prefs__form,
  .tp-prefs__field {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-prefs__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-prefs__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-prefs__choices {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-4);
  }

  .tp-prefs__choice-set {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }

  .tp-prefs__choice {
    display: flex;
    align-items: center;
    gap: var(--tp-space-1);
    color: var(--tp-text);
  }

  .tp-prefs__error {
    margin: 0;
    padding-left: var(--tp-space-2);
    border-left: 3px solid var(--tp-status-danger);
    color: var(--tp-text-strong);
  }

  .tp-prefs__notice {
    margin: 0;
    padding-left: var(--tp-space-2);
    border-left: 3px solid var(--tp-status-success);
    color: var(--tp-text-strong);
  }

  .tp-prefs__about {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--tp-space-1) var(--tp-space-3);
    margin: 0;
  }

  .tp-prefs__about dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-prefs__about dd {
    margin: 0;
    color: var(--tp-text);
  }
</style>
