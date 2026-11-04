<script lang="ts">
  /**
   * The bundle as files: pick one, edit its exact text, add or remove a path.
   *
   * This is a text editor, not a form. The textarea holds the file's exact
   * bytes and an edit replaces only that file's content; the page's model
   * guarantees the other files are sent exactly as they were stored. The only
   * structural changes offered are adding a path at the end and removing one
   * path.
   */
  import IssueList from './IssueList.svelte';
  import FormField from '../../components/FormField.svelte';
  import type { DraftFile, LocatedIssue } from '$lib/configuration';

  interface Props {
    /** The draft files, in manifest order. */
    files?: readonly DraftFile[];
    /** The file currently open in the editor. */
    selectedPath?: string | null;
    /** Field issues located against the draft. */
    issues?: readonly LocatedIssue[];
    /** True while a pool call is in flight. */
    busy?: boolean;
    /** False for a role the contract will refuse; the textarea is read-only. */
    canEdit?: boolean;
    /** Open one file. */
    onSelect?: (path: string) => void;
    /** Replace one file's content. */
    onEdit?: (path: string, content: string) => void;
    /** Add a path at the end of the bundle. */
    onAdd?: (path: string) => void;
    /** Remove one path. */
    onRemove?: (path: string) => void;
  }

  let {
    files = [],
    selectedPath = null,
    issues = [],
    busy = false,
    canEdit = true,
    onSelect,
    onEdit,
    onAdd,
    onRemove,
  }: Props = $props();

  let newPath = $state('');

  const current = $derived(
    files.find((file) => file.path === selectedPath) ?? files[0] ?? null,
  );
  const canRemove = $derived(files.length > 1);

  /** Add the typed path, then clear the field. */
  function addFile(): void {
    const path = newPath.trim();
    if (path === '' || !onAdd) return;
    onAdd(path);
    newPath = '';
  }
</script>

<section class="tp-editor" aria-labelledby="tp-editor-title" data-configuration-editor="true">
  <h3 class="tp-editor__title" id="tp-editor-title">Files this project's runs use</h3>

  {#if files.length === 0}
    <p class="tp-page__state" data-editor-empty="true">
      This project has no files yet. Start with the manifest: add
      <code>tidepool.json</code> to list what a run uses, then add each prompt
      file it names.
    </p>
  {:else}
    <nav class="tp-editor__files" aria-label="Bundle files">
      <ul class="tp-editor__file-list">
        {#each files as file (file.path)}
          <li>
            <button
              class="tp-editor__file"
              type="button"
              aria-current={file.path === current?.path ? 'true' : undefined}
              data-file-path={file.path}
              data-file-dirty={file.dirty ? 'true' : 'false'}
              onclick={() => onSelect?.(file.path)}
            >
              <code>{file.path}</code>
              {#if file.dirty}
                <span class="tp-editor__dirty" aria-label="edited">edited</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    </nav>

    {#if current}
      <div class="tp-editor__panel">
        <div class="tp-editor__head">
          <h4 class="tp-editor__path" data-editor-path={current.path}>
            <code>{current.path}</code>
          </h4>
          {#if onRemove}
            <button
              class="tp-button"
              type="button"
              disabled={busy || !canEdit || !canRemove}
              data-editor-action="remove"
              onclick={() => onRemove?.(current.path)}
            >
              Remove file
            </button>
          {/if}
        </div>
        <FormField
          form="file_editor"
          field="content"
          label={`Exact content of ${current.path}`}
          required
          id="tp-editor-textarea"
        >
          {#snippet children(id, described)}
            <textarea
              class="tp-field__control tp-editor__textarea"
              {id}
              name="configuration-file"
              spellcheck="false"
              readonly={!canEdit || busy}
              rows="18"
              aria-describedby={described}
              data-file-content-path={current.path}
              oninput={(event) => onEdit?.(current.path, event.currentTarget.value)}
            >{current.content}</textarea>
          {/snippet}
        </FormField>
      </div>
    {/if}
  {/if}

  {#if onAdd}
    <form
      class="tp-editor__add"
      data-form="add_file_path"
      onsubmit={(event) => {
        event.preventDefault();
        addFile();
      }}
    >
      <FormField
        form="add_file_path"
        field="path"
        label="Add a file path"
        required
        id="tp-editor-new-path"
        help="Relative to the project root, e.g. prompts/system.md."
      >
        {#snippet children(id, described)}
          <input
            class="tp-field__control"
            {id}
            name="new-file-path"
            type="text"
            placeholder="prompts/system.md"
            aria-describedby={described}
            bind:value={newPath}
            disabled={busy || !canEdit}
          />
        {/snippet}
      </FormField>
      <div class="tp-field__actions">
        <button
          class="tp-button"
          type="submit"
          disabled={busy || !canEdit || newPath.trim() === ''}
          data-editor-action="add"
        >
          Add file
        </button>
      </div>
    </form>
  {/if}

  <IssueList {issues} />
</section>

<style>
  .tp-editor {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-editor__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
  }

  .tp-editor__file-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-editor__file {
    display: inline-flex;
    gap: var(--tp-space-1);
    align-items: baseline;
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-chip);
    background: transparent;
    color: var(--tp-text);
    cursor: pointer;
  }

  .tp-editor__file[aria-current='true'] {
    border-color: var(--tp-primary);
  }

  .tp-editor__dirty {
    color: var(--tp-status-warning);
    font-family: var(--tp-font-caption);
  }

  .tp-editor__panel {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-editor__head {
    display: flex;
    gap: var(--tp-space-2);
    justify-content: space-between;
    align-items: baseline;
  }

  .tp-editor__path {
    margin: 0;
    font-size: var(--tp-font-body);
  }

  .tp-editor__textarea {
    width: 100%;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text);
    font-family: var(--tp-font-code);
    font-size: var(--tp-font-caption);
    line-height: 1.5;
    resize: vertical;
  }

  .tp-editor__add {
    display: grid;
    gap: var(--tp-space-1);
  }

</style>
