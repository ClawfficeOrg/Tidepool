<script lang="ts">
  /**
   * The trigger form.
   *
   * It edits one document through typed `TriggerEdit` values. Host and
   * directory are separate because changing either reloads the host's agent
   * choices; every other field is a plain edit. An authored agent value the
   * host no longer reports stays in the select, labelled unavailable, so the
   * author chooses the replacement rather than the page choosing silently.
   */
  import type {
    AgentOptions,
    SelectionAvailability,
    TriggerEdit,
    TriggerHostRow,
  } from '$lib/triggers';
  import {
    agentThinkingKey,
    agentVariantKey,
    EVENT_PROVIDERS,
  } from '$lib/triggers';
  import type { TriggerDocument, TriggerIssue } from '@clawffice/tidepool-sdk';

  interface Props {
    /** The canonical document being edited. */
    document: TriggerDocument;
    /** Enrolled hosts, runnable first. */
    hosts: readonly TriggerHostRow[];
    /** The working directory; not written into the document. */
    directory: string;
    /** The host's offered choices, or null while they are unavailable. */
    options: AgentOptions | null;
    /** Which authored values the host offers. */
    availability: SelectionAvailability;
    /** Named issues to mark beside their fields. */
    issues: readonly TriggerIssue[];
    /** False for a viewer, whose edits the pool would refuse. */
    disabled?: boolean;
    /** Apply one field edit. */
    onEdit: (edit: TriggerEdit) => void;
    /** Choose the host and reload its choices. */
    onHost: (slug: string) => void;
    /** Change the working directory and reload the choices. */
    onDirectory: (directory: string) => void;
  }

  let {
    document,
    hosts,
    directory,
    options,
    availability,
    issues,
    disabled = false,
    onEdit,
    onHost,
    onDirectory,
  }: Props = $props();

  const invalid = $derived(new Set(issues.map((issue) => issue.field)));
  const providerChoices = $derived(options?.providers ?? []);
  const modelChoices = $derived(
    options === null ? [] : options.models[document.run.agent.provider] ?? [],
  );
  const modeChoices = $derived(
    options === null
      ? []
      : options.modes[
          agentVariantKey(document.run.agent.provider, document.run.agent.model)
        ] ?? [],
  );
  const thinkingChoices = $derived(
    options === null
      ? []
      : options.thinking[
          agentThinkingKey(
            document.run.agent.provider,
            document.run.agent.model,
            document.run.agent.execution_mode,
          )
        ] ?? [],
  );

  /** The host slugs, with an authored unknown slug kept and labelled. */
  const hostChoices = $derived.by(() => {
    const known = hosts.map((host) => ({
      value: host.slug,
      label: host.runnable
        ? `${host.slug} — ${host.displayName}`
        : `${host.slug} — ${host.status.replace('_', ' ')}`,
    }));
    const current = document.run.host;
    if (current.trim() !== '' && !hosts.some((host) => host.slug === current)) {
      return [{ value: current, label: `${current} — unknown host` }, ...known];
    }
    return known;
  });

  /** Values for a select, keeping an authored value the host no longer offers. */
  function selectable(
    values: readonly string[],
    current: string,
    markUnavailable = options !== null,
  ): {
    value: string;
    label: string;
  }[] {
    const offered = values.map((value) => ({ value, label: value }));
    if (current.trim() !== '' && !values.includes(current)) {
      const suffix = markUnavailable ? '(unavailable)' : '(not checked)';
      return [{ value: current, label: `${current} ${suffix}` }, ...offered];
    }
    return offered;
  }

  function valueOf(event: Event): string {
    return (event.currentTarget as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement)
      .value;
  }

  function optionalValueOf(event: Event): string | null {
    const value = valueOf(event).trim();
    return value === '' ? null : value;
  }

  function usersOf(event: Event): string[] {
    return valueOf(event)
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '');
  }

  function fieldInvalid(field: string): boolean {
    return invalid.has(field);
  }
</script>

<fieldset class="tp-trigger-form" disabled={disabled} data-trigger-form="true">
  <legend class="tp-trigger-form__legend">Trigger</legend>

  <label class="tp-field">
    <span class="tp-field__label">Name</span>
    <input
      class="tp-input"
      type="text"
      value={document.name}
      data-field="name"
      aria-invalid={fieldInvalid('name')}
      oninput={(event) => onEdit({ field: 'name', value: valueOf(event) })}
    />
  </label>

  <div class="tp-trigger-form__row">
    <label class="tp-field">
      <span class="tp-field__label">Event provider</span>
      <select
        class="tp-input"
        value={document.event.provider}
        data-field="event.provider"
        aria-invalid={fieldInvalid('event.provider')}
        onchange={(event) => onEdit({ field: 'event.provider', value: valueOf(event) })}
      >
        {#each EVENT_PROVIDERS as provider (provider)}
          <option value={provider}>{provider}</option>
        {/each}
      </select>
    </label>
    <label class="tp-field">
      <span class="tp-field__label">Event type</span>
      <input
        class="tp-input"
        type="text"
        value={document.event.type}
        placeholder="issue.comment_created"
        data-field="event.type"
        aria-invalid={fieldInvalid('event.type')}
        oninput={(event) => onEdit({ field: 'event.type', value: valueOf(event) })}
      />
    </label>
  </div>

  <label class="tp-field">
    <span class="tp-field__label">Connection slug</span>
    <input
      class="tp-input"
      type="text"
      value={document.filters.connection}
      data-field="filters.connection"
      aria-invalid={fieldInvalid('filters.connection')}
      oninput={(event) => onEdit({ field: 'filters.connection', value: valueOf(event) })}
    />
  </label>

  <div class="tp-trigger-form__row">
    <label class="tp-field">
      <span class="tp-field__label">Resource kind</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.resource.kind}
        data-field="filters.resource.kind"
        aria-invalid={fieldInvalid('filters.resource.kind')}
        oninput={(event) =>
          onEdit({ field: 'filters.resource.kind', value: valueOf(event) })}
      />
    </label>
    <label class="tp-field">
      <span class="tp-field__label">Resource id</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.resource.id}
        placeholder="acme/widgets"
        data-field="filters.resource.id"
        aria-invalid={fieldInvalid('filters.resource.id')}
        oninput={(event) => onEdit({ field: 'filters.resource.id', value: valueOf(event) })}
      />
    </label>
  </div>

  <label class="tp-field">
    <span class="tp-field__label">Allowed users</span>
    <textarea
      class="tp-input tp-trigger-form__users"
      rows="3"
      value={document.filters.from_users.join('\n')}
      placeholder="One handle per line. Required: the allowlist is what keeps a stranger's event from starting an agent."
      data-field="filters.from_users"
      aria-invalid={fieldInvalid('filters.from_users')}
      oninput={(event) => onEdit({ field: 'filters.from_users', value: usersOf(event) })}
    ></textarea>
    <span class="tp-field__hint">
      Required and never defaulted. Every handle is one person allowed to start
      this run.
    </span>
  </label>

  <div class="tp-trigger-form__row">
    <label class="tp-field">
      <span class="tp-field__label">Channel (optional)</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.channel ?? ''}
        data-field="filters.channel"
        aria-invalid={fieldInvalid('filters.channel')}
        oninput={(event) =>
          onEdit({ field: 'filters.channel', value: optionalValueOf(event) })}
      />
    </label>
    <label class="tp-field">
      <span class="tp-field__label">Label (optional)</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.label ?? ''}
        data-field="filters.label"
        aria-invalid={fieldInvalid('filters.label')}
        oninput={(event) => onEdit({ field: 'filters.label', value: optionalValueOf(event) })}
      />
    </label>
  </div>

  <div class="tp-trigger-form__row">
    <label class="tp-field">
      <span class="tp-field__label">Contains (optional)</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.contains ?? ''}
        data-field="filters.contains"
        aria-invalid={fieldInvalid('filters.contains')}
        oninput={(event) =>
          onEdit({ field: 'filters.contains', value: optionalValueOf(event) })}
      />
    </label>
    <label class="tp-field">
      <span class="tp-field__label">Pattern (optional)</span>
      <input
        class="tp-input"
        type="text"
        value={document.filters.pattern ?? ''}
        data-field="filters.pattern"
        aria-invalid={fieldInvalid('filters.pattern')}
        oninput={(event) => onEdit({ field: 'filters.pattern', value: optionalValueOf(event) })}
      />
    </label>
  </div>

  <div class="tp-trigger-form__row">
    <label class="tp-field">
      <span class="tp-field__label">Host</span>
      <select
        class="tp-input"
        value={document.run.host}
        data-field="run.host"
        aria-invalid={fieldInvalid('run.host')}
        onchange={(event) => onHost(valueOf(event))}
      >
        <option value="">Choose a host…</option>
        {#each hostChoices as host (host.value)}
          <option value={host.value}>{host.label}</option>
        {/each}
      </select>
    </label>
    <label class="tp-field">
      <span class="tp-field__label">Working directory</span>
      <input
        class="tp-input"
        type="text"
        value={directory}
        placeholder="/srv/tidepool"
        data-field="directory"
        onchange={(event) => onDirectory(valueOf(event))}
      />
      <span class="tp-field__hint">
        The context the host is asked about. This contract stores the host and
        agent in the trigger; the directory is not part of the document.
      </span>
    </label>
  </div>

  <fieldset class="tp-trigger-form__agent" data-agent-fields="true">
    <legend class="tp-trigger-form__legend">Agent</legend>

    <div class="tp-trigger-form__row">
      <label class="tp-field">
        <span class="tp-field__label">Provider</span>
        <select
          class="tp-input"
          value={document.run.agent.provider}
          data-field="run.agent.provider"
          aria-invalid={fieldInvalid('run.agent.provider')}
          onchange={(event) =>
            onEdit({ field: 'run.agent.provider', value: valueOf(event) })}
        >
          {#if document.run.agent.provider.trim() === ''}
            <option value="">Choose a host to load providers…</option>
          {/if}
          {#each selectable(providerChoices, document.run.agent.provider) as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </select>
      </label>
      <label class="tp-field">
        <span class="tp-field__label">Model</span>
        <select
          class="tp-input"
          value={document.run.agent.model}
          data-field="run.agent.model"
          aria-invalid={fieldInvalid('run.agent.model')}
          onchange={(event) => onEdit({ field: 'run.agent.model', value: valueOf(event) })}
        >
          {#if document.run.agent.model.trim() === ''}
            <option value="">Choose a provider first…</option>
          {/if}
          {#each selectable(modelChoices, document.run.agent.model) as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </select>
      </label>
    </div>

    <div class="tp-trigger-form__row">
      <label class="tp-field">
        <span class="tp-field__label">Execution mode</span>
        <select
          class="tp-input"
          value={document.run.agent.execution_mode}
          data-field="run.agent.execution_mode"
          aria-invalid={fieldInvalid('run.agent.execution_mode')}
          onchange={(event) =>
            onEdit({ field: 'run.agent.execution_mode', value: valueOf(event) })}
        >
          {#if document.run.agent.execution_mode.trim() === ''}
            <option value="">Choose a model first…</option>
          {/if}
          {#each selectable(modeChoices, document.run.agent.execution_mode) as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </select>
      </label>
      <label class="tp-field">
        <span class="tp-field__label">Thinking option</span>
        <select
          class="tp-input"
          value={document.run.agent.thinking_option ?? ''}
          data-field="run.agent.thinking_option"
          aria-invalid={fieldInvalid('run.agent.thinking_option')}
          onchange={(event) => {
            const value = valueOf(event);
            onEdit({ field: 'run.agent.thinking_option', value: value === '' ? null : value });
          }}
        >
          <option value="">No thinking option</option>
          {#each selectable(thinkingChoices, document.run.agent.thinking_option ?? '') as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </select>
      </label>
    </div>

    {#if availability.unavailable.length > 0}
      <p class="tp-trigger-form__unavailable" data-agent-unavailable-fields="true">
        The host no longer reports
        {availability.unavailable
          .map((entry) => `${entry.field} '${entry.value ?? ''}'`)
          .join(', ')}. The authored value is kept; choose a reported value or
        move to a host that offers it.
      </p>
    {/if}
  </fieldset>
</fieldset>

<style>
  .tp-trigger-form {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-trigger-form__legend {
    padding: 0 var(--tp-space-1);
    font-family: var(--tp-font-heading);
  }

  .tp-trigger-form__row {
    display: grid;
    gap: var(--tp-space-2);
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  }

  .tp-trigger-form__agent {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }

  .tp-trigger-form__users {
    min-height: 4rem;
    font-family: var(--tp-font-code);
  }

  .tp-field {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-field__label {
    font-size: var(--tp-font-caption);
    color: var(--tp-text-muted);
  }

  .tp-field__hint {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-trigger-form__unavailable {
    margin: 0;
    color: var(--tp-status-warning);
  }
</style>
