<script lang="ts">
  /**
   * The trigger form.
   *
   * It edits one document through typed `TriggerEdit` values. Host and
   * directory are separate because changing either reloads the host's agent
   * choices; every other field is a plain edit. An authored agent value the
   * host no longer reports stays in the select, labelled unavailable, so the
   * author chooses the replacement rather than the page choosing silently.
   *
   * Every field is a `FormField`, so the label, requirement marker and any
   * named issue sit in the one place, in the one order.
   */
  import FormField from '../../components/FormField.svelte';
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

  /** The message for a field, when the page named one against it. */
  function fieldError(field: string): string | null {
    return issues.find((issue) => issue.field === field)?.message ?? null;
  }
</script>

<fieldset class="tp-trigger-form" disabled={disabled} data-trigger-form="true">
  <legend class="tp-trigger-form__legend">Trigger</legend>

  <FormField form="trigger" field="name" label="Name" required error={fieldError('name')}>
    {#snippet children(id, described)}
      <input
        class="tp-field__control"
        {id}
        type="text"
        value={document.name}
        aria-invalid={fieldInvalid('name')}
        aria-describedby={described}
        data-field="name"
        oninput={(event) => onEdit({ field: 'name', value: valueOf(event) })}
      />
    {/snippet}
  </FormField>

  <div class="tp-trigger-form__row">
    <FormField
      form="trigger"
      field="event.provider"
      label="Event provider"
      required
      error={fieldError('event.provider')}
    >
      {#snippet children(id, described)}
        <select
          class="tp-field__control"
          {id}
          value={document.event.provider}
          aria-invalid={fieldInvalid('event.provider')}
          aria-describedby={described}
          data-field="event.provider"
          onchange={(event) => onEdit({ field: 'event.provider', value: valueOf(event) })}
        >
          {#each EVENT_PROVIDERS as provider (provider)}
            <option value={provider}>{provider}</option>
          {/each}
        </select>
      {/snippet}
    </FormField>

    <FormField
      form="trigger"
      field="event.type"
      label="Event type"
      required
      error={fieldError('event.type')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.event.type}
          placeholder="issue.comment_created"
          aria-invalid={fieldInvalid('event.type')}
          aria-describedby={described}
          data-field="event.type"
          oninput={(event) => onEdit({ field: 'event.type', value: valueOf(event) })}
        />
      {/snippet}
    </FormField>
  </div>

  <FormField
    form="trigger"
    field="filters.connection"
    label="Connection slug"
    required
    error={fieldError('filters.connection')}
  >
    {#snippet children(id, described)}
      <input
        class="tp-field__control"
        {id}
        type="text"
        value={document.filters.connection}
        aria-invalid={fieldInvalid('filters.connection')}
        aria-describedby={described}
        data-field="filters.connection"
        oninput={(event) => onEdit({ field: 'filters.connection', value: valueOf(event) })}
      />
    {/snippet}
  </FormField>

  <div class="tp-trigger-form__row">
    <FormField
      form="trigger"
      field="filters.resource.kind"
      label="Resource kind"
      required
      error={fieldError('filters.resource.kind')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.resource.kind}
          aria-invalid={fieldInvalid('filters.resource.kind')}
          aria-describedby={described}
          data-field="filters.resource.kind"
          oninput={(event) =>
            onEdit({ field: 'filters.resource.kind', value: valueOf(event) })}
        />
      {/snippet}
    </FormField>

    <FormField
      form="trigger"
      field="filters.resource.id"
      label="Resource id"
      required
      error={fieldError('filters.resource.id')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.resource.id}
          placeholder="acme/widgets"
          aria-invalid={fieldInvalid('filters.resource.id')}
          aria-describedby={described}
          data-field="filters.resource.id"
          oninput={(event) => onEdit({ field: 'filters.resource.id', value: valueOf(event) })}
        />
      {/snippet}
    </FormField>
  </div>

  <FormField
    form="trigger"
    field="filters.from_users"
    label="Allowed users"
    required
    help="Required and never defaulted. Every handle is one person allowed to start this run."
    error={fieldError('filters.from_users')}
  >
    {#snippet children(id, described)}
      <textarea
        class="tp-field__control tp-trigger-form__users"
        {id}
        rows="3"
        value={document.filters.from_users.join('\n')}
        placeholder="One handle per line. Required: the allowlist is what keeps a stranger's event from starting an agent."
        aria-invalid={fieldInvalid('filters.from_users')}
        aria-describedby={described}
        data-field="filters.from_users"
        oninput={(event) => onEdit({ field: 'filters.from_users', value: usersOf(event) })}
      ></textarea>
    {/snippet}
  </FormField>

  <div class="tp-trigger-form__row">
    <FormField
      form="trigger"
      field="filters.channel"
      label="Channel"
      error={fieldError('filters.channel')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.channel ?? ''}
          aria-invalid={fieldInvalid('filters.channel')}
          aria-describedby={described}
          data-field="filters.channel"
          oninput={(event) =>
            onEdit({ field: 'filters.channel', value: optionalValueOf(event) })}
        />
      {/snippet}
    </FormField>

    <FormField form="trigger" field="filters.label" label="Label" error={fieldError('filters.label')}>
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.label ?? ''}
          aria-invalid={fieldInvalid('filters.label')}
          aria-describedby={described}
          data-field="filters.label"
          oninput={(event) => onEdit({ field: 'filters.label', value: optionalValueOf(event) })}
        />
      {/snippet}
    </FormField>
  </div>

  <div class="tp-trigger-form__row">
    <FormField
      form="trigger"
      field="filters.contains"
      label="Contains"
      error={fieldError('filters.contains')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.contains ?? ''}
          aria-invalid={fieldInvalid('filters.contains')}
          aria-describedby={described}
          data-field="filters.contains"
          oninput={(event) =>
            onEdit({ field: 'filters.contains', value: optionalValueOf(event) })}
        />
      {/snippet}
    </FormField>

    <FormField
      form="trigger"
      field="filters.pattern"
      label="Pattern"
      error={fieldError('filters.pattern')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={document.filters.pattern ?? ''}
          aria-invalid={fieldInvalid('filters.pattern')}
          aria-describedby={described}
          data-field="filters.pattern"
          oninput={(event) => onEdit({ field: 'filters.pattern', value: optionalValueOf(event) })}
        />
      {/snippet}
    </FormField>
  </div>

  <div class="tp-trigger-form__row">
    <FormField form="trigger" field="run.host" label="Host" required error={fieldError('run.host')}>
      {#snippet children(id, described)}
        <select
          class="tp-field__control"
          {id}
          value={document.run.host}
          aria-invalid={fieldInvalid('run.host')}
          aria-describedby={described}
          data-field="run.host"
          onchange={(event) => onHost(valueOf(event))}
        >
          <option value="">Choose a host…</option>
          {#each hostChoices as host (host.value)}
            <option value={host.value}>{host.label}</option>
          {/each}
        </select>
      {/snippet}
    </FormField>

    <FormField
      form="trigger"
      field="directory"
      label="Working directory"
      help="The context the host is asked about. This contract stores the host and agent in the trigger; the directory is not part of the document."
      error={fieldError('directory')}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          value={directory}
          placeholder="/srv/tidepool"
          aria-describedby={described}
          data-field="directory"
          onchange={(event) => onDirectory(valueOf(event))}
        />
      {/snippet}
    </FormField>
  </div>

  <fieldset class="tp-trigger-form__agent" data-agent-fields="true">
    <legend class="tp-trigger-form__legend">Agent</legend>

    <div class="tp-trigger-form__row">
      <FormField
        form="trigger"
        field="run.agent.provider"
        label="Provider"
        required
        error={fieldError('run.agent.provider')}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            value={document.run.agent.provider}
            aria-invalid={fieldInvalid('run.agent.provider')}
            aria-describedby={described}
            data-field="run.agent.provider"
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
        {/snippet}
      </FormField>

      <FormField
        form="trigger"
        field="run.agent.model"
        label="Model"
        required
        error={fieldError('run.agent.model')}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            value={document.run.agent.model}
            aria-invalid={fieldInvalid('run.agent.model')}
            aria-describedby={described}
            data-field="run.agent.model"
            onchange={(event) => onEdit({ field: 'run.agent.model', value: valueOf(event) })}
          >
            {#if document.run.agent.model.trim() === ''}
              <option value="">Choose a provider first…</option>
            {/if}
            {#each selectable(modelChoices, document.run.agent.model) as choice (choice.value)}
              <option value={choice.value}>{choice.label}</option>
            {/each}
          </select>
        {/snippet}
      </FormField>
    </div>

    <div class="tp-trigger-form__row">
      <FormField
        form="trigger"
        field="run.agent.execution_mode"
        label="Execution mode"
        required
        error={fieldError('run.agent.execution_mode')}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            value={document.run.agent.execution_mode}
            aria-invalid={fieldInvalid('run.agent.execution_mode')}
            aria-describedby={described}
            data-field="run.agent.execution_mode"
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
        {/snippet}
      </FormField>

      <FormField
        form="trigger"
        field="run.agent.thinking_option"
        label="Thinking option"
        error={fieldError('run.agent.thinking_option')}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            value={document.run.agent.thinking_option ?? ''}
            aria-invalid={fieldInvalid('run.agent.thinking_option')}
            aria-describedby={described}
            data-field="run.agent.thinking_option"
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
        {/snippet}
      </FormField>
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

  .tp-trigger-form__unavailable {
    margin: 0;
    color: var(--tp-status-warning);
  }
</style>
