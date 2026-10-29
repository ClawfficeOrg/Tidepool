/**
 * The configuration editor's model.
 *
 * A project's configuration is a bundle of authored files, stored verbatim:
 * `tidepool.json` is the manifest and the rest are the prompt partials it
 * references. Nothing in this module normalises, reorders or reformats a file
 * — an edit replaces the exact bytes of one path and leaves every other file
 * byte-for-byte as the author wrote it. That is the point of the editor: it is
 * a text editor over real authored files, not a form that regenerates a
 * document from a model.
 *
 * # Validation is the pool's, not ours
 *
 * `save` calls the same validate endpoint the pool uses, then installs only a
 * bundle that validated. The pool answers with named field issues
 * (`hosts.studio-mac`, `environments.nightly.directory`,
 * `workflows.build.steps[2].agent`); [`locateIssues`] resolves each one to the
 * exact line of the authored manifest by parsing the manifest with byte
 * offsets, so the editor can put the message beside the offending line instead
 * of beside a field name a person has to search for.
 *
 * The mapping degrades honestly. A field the parser cannot find is marked
 * `located: false` and pinned to the manifest's first line rather than being
 * silently dropped or pointed at a guessed line.
 *
 * # The active revision is unambiguous
 *
 * The pool marks exactly one revision active and the history carries the flag.
 * [`resolveActiveRevision`] refuses to guess: zero active revisions when a
 * history exists, more than one active, or a served active revision that the
 * history does not mark active are all reported as an explicit problem. A
 * failed save never touches the active revision; the refusal path keeps the
 * previous one exactly as it was.
 *
 * # Diffs are over authored files
 *
 * [`diffBundles`] compares the authored file sets, including file order, so a
 * diff between revisions shows the files the author wrote — added, removed or
 * changed — with a line diff inside each changed file.
 */

import type {
  ConfigurationRequest,
  FieldIssue,
  InstallResponse,
  ProjectView,
  ProjectsResponse,
  RevisionMetaView,
  RevisionView,
  RevisionsResponse,
  TidepoolClient,
  ValidateResponse,
} from '@clawffice/tidepool-sdk';
import { isProblemError } from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, type PoolConfig } from './api/pool';

/** The manifest's fixed path inside every bundle. */
export const MANIFEST_FILE = 'tidepool.json';

/** The contract's maximum number of files in a bundle. */
export const MAX_BUNDLE_FILES = 100;

/** The most lines a single file diff will align before it summarizes instead. */
export const MAX_DIFF_LINES = 2000;

/** One authored file: an exact path and its verbatim content. */
export interface AuthoredFile {
  /** Path relative to the bundle root, e.g. `prompts/system.md`. */
  path: string;
  /** The bytes the author wrote, stored and sent unchanged. */
  content: string;
}

/** An authored file bundle, in the order the author listed the files. */
export interface Bundle {
  /** Files in authored order. */
  files: AuthoredFile[];
}

/** One file in the editor, with whether it differs from the active revision. */
export interface DraftFile extends AuthoredFile {
  /** True when the file is new or edited since the active revision. */
  dirty: boolean;
}

/** A position in an authored file, 1-based like every editor. */
export interface FilePosition {
  /** Line number, 1-based. */
  line: number;
  /** Column number, 1-based. */
  column: number;
}

/** Where one field issue points in the authored text. */
export interface FieldLocation extends FilePosition {
  /** True when the parser resolved the field to a real node. */
  located: boolean;
  /** The manifest line's text, trimmed for display. */
  snippet: string;
}

/** A field issue with the file and line it belongs to. */
export interface LocatedIssue extends FieldIssue, FieldLocation {
  /** The authored file the issue points into; the manifest in this contract. */
  file: string;
}

/** The empty bundle an unconfigured project starts from. */
export function emptyBundle(): Bundle {
  return { files: [] };
}

/** A starter bundle with a parseable, empty manifest. */
export function starterBundle(): Bundle {
  return { files: [{ path: MANIFEST_FILE, content: '{\n}\n' }] };
}

/** Copy a revision's authored files, preserving their stored order. */
export function bundleFromFiles(files: readonly AuthoredFile[]): Bundle {
  return { files: files.map((file) => ({ path: file.path, content: file.content })) };
}

/** The authored bundle of a stored revision. */
export function bundleFromRevision(revision: RevisionView): Bundle {
  return bundleFromFiles(revision.files);
}

/** The manifest file in a bundle, if there is one. */
export function findManifest(bundle: Bundle): AuthoredFile | null {
  return bundle.files.find((file) => file.path === MANIFEST_FILE) ?? null;
}

/** One file by path. */
export function findFile(bundle: Bundle, path: string): AuthoredFile | null {
  return bundle.files.find((file) => file.path === path) ?? null;
}

/**
 * Replace one file's exact content.
 *
 * The bundle keeps its order and every untouched file keeps its exact
 * content — no parse, no stringify, no reindent. The object for untouched
 * files is shared with the input bundle, which is how the test proves nothing
 * else was rewritten. A path that is not in the bundle returns the bundle
 * unchanged.
 */
export function editBundleFile(bundle: Bundle, path: string, content: string): Bundle {
  if (!bundle.files.some((file) => file.path === path)) return bundle;
  return {
    files: bundle.files.map((file) =>
      file.path === path ? { path: file.path, content } : file,
    ),
  };
}

/** Add a file at the end of the bundle; an existing path is left untouched. */
export function addBundleFile(bundle: Bundle, path: string, content = ''): Bundle {
  if (bundle.files.some((file) => file.path === path)) return bundle;
  return { files: [...bundle.files, { path, content }] };
}

/** Remove one file; every other file keeps its order and content. */
export function removeBundleFile(bundle: Bundle, path: string): Bundle {
  if (!bundle.files.some((file) => file.path === path)) return bundle;
  return { files: bundle.files.filter((file) => file.path !== path) };
}

/** Every authored path, in authored order. */
export function bundlePaths(bundle: Bundle): string[] {
  return bundle.files.map((file) => file.path);
}

/**
 * The request body for validate/install.
 *
 * The files go out in authored order with their exact content. There is no
 * normalisation step here on purpose.
 */
export function requestFromBundle(bundle: Bundle): ConfigurationRequest {
  return {
    files: bundle.files.map((file) => ({ path: file.path, content: file.content })),
  };
}

/** The files in the draft, each marked dirty against the active revision. */
export function draftFiles(draft: Bundle, active: RevisionView | null): DraftFile[] {
  const stored = new Map((active?.files ?? []).map((file) => [file.path, file.content]));
  return draft.files.map((file) => ({
    path: file.path,
    content: file.content,
    dirty: stored.get(file.path) !== file.content,
  }));
}

/** Paths in the draft that differ from the active revision, in draft order. */
export function dirtyPaths(draft: Bundle, active: RevisionView | null): string[] {
  return draftFiles(draft, active)
    .filter((file) => file.dirty)
    .map((file) => file.path);
}

// ── locating a field issue in authored JSON ───────────────────────────────

/** A parsed JSON node, with source offsets but no normalised value. */
interface JsonNode {
  kind: 'object' | 'array' | 'string' | 'number' | 'boolean' | 'null';
  start: number;
  end: number;
  members?: { key: string; value: JsonNode }[];
  items?: JsonNode[];
}

/** Parse JSON while keeping source offsets. Returns null for malformed input. */
function parseJsonDocument(text: string): JsonNode | null {
  let index = 0;
  let depth = 0;

  const fail = (): never => {
    throw new SyntaxError('invalid JSON');
  };

  const skipWhitespace = (): void => {
    while (index < text.length && /\s/.test(text[index] ?? '')) index += 1;
  };

  const parseString = (): JsonNode => {
    const start = index;
    if (text[index] !== '"') fail();
    index += 1;
    while (index < text.length) {
      const char = text[index];
      if (char === '\\') {
        index += 2;
        continue;
      }
      index += 1;
      if (char === '"') return { kind: 'string', start, end: index };
    }
    return fail();
  };

  const parseValue = (): JsonNode => {
    depth += 1;
    if (depth > 128) fail();
    skipWhitespace();
    const start = index;
    const char = text[index];
    if (char === '{') {
      index += 1;
      const members: { key: string; value: JsonNode }[] = [];
      skipWhitespace();
      if (text[index] === '}') {
        index += 1;
        depth -= 1;
        return { kind: 'object', start, end: index, members };
      }
      for (;;) {
        skipWhitespace();
        const keyNode = parseString();
        const key = JSON.parse(text.slice(keyNode.start, keyNode.end)) as string;
        skipWhitespace();
        if (text[index] !== ':') fail();
        index += 1;
        const value = parseValue();
        members.push({ key, value });
        skipWhitespace();
        const separator = text[index];
        if (separator === ',') {
          index += 1;
          continue;
        }
        if (separator === '}') {
          index += 1;
          depth -= 1;
          return { kind: 'object', start, end: index, members };
        }
        fail();
      }
    }
    if (char === '[') {
      index += 1;
      const items: JsonNode[] = [];
      skipWhitespace();
      if (text[index] === ']') {
        index += 1;
        depth -= 1;
        return { kind: 'array', start, end: index, items };
      }
      for (;;) {
        items.push(parseValue());
        skipWhitespace();
        const separator = text[index];
        if (separator === ',') {
          index += 1;
          continue;
        }
        if (separator === ']') {
          index += 1;
          depth -= 1;
          return { kind: 'array', start, end: index, items };
        }
        fail();
      }
    }
    if (char === '"') {
      const node = parseString();
      depth -= 1;
      return node;
    }
    for (const literal of ['true', 'false', 'null'] as const) {
      if (text.startsWith(literal, index)) {
        index += literal.length;
        depth -= 1;
        return { kind: literal === 'null' ? 'null' : 'boolean', start, end: index };
      }
    }
    while (index < text.length && /[-+0-9.eE]/.test(text[index] ?? '')) index += 1;
    if (index === start) fail();
    depth -= 1;
    return { kind: 'number', start, end: index };
  };

  try {
    const root = parseValue();
    skipWhitespace();
    if (index !== text.length) return null;
    return root;
  } catch {
    return null;
  }
}

/** The decoded string value of a node, when it is a string. */
function stringValue(text: string, node: JsonNode): string | null {
  if (node.kind !== 'string') return null;
  try {
    return JSON.parse(text.slice(node.start, node.end)) as string;
  } catch {
    return null;
  }
}

/** A string member of an object node, e.g. an array item's `name`. */
function stringMember(text: string, node: JsonNode, key: string): string | null {
  if (node.kind !== 'object' || !node.members) return null;
  const member = node.members.find((candidate) => candidate.key === key);
  return member ? stringValue(text, member.value) : null;
}

/**
 * Index every plausible field path to its value's source offset.
 *
 * Named arrays (workflows, environments, agents, partials) are indexed by both
 * name and position, because the pool's field paths use the name for the
 * former and `[index]` for the latter. String-array items (hosts, connections)
 * are indexed by value, which is how `hosts.studio-mac` finds its line.
 */
function indexDocument(text: string, root: JsonNode): Map<string, number> {
  const index = new Map<string, number>();

  const add = (path: string, offset: number): void => {
    if (path !== '' && !index.has(path)) index.set(path, offset);
  };

  const walk = (node: JsonNode, path: string): void => {
    if (node.kind === 'object' && node.members) {
      for (const member of node.members) {
        const childPath = path === '' ? member.key : `${path}.${member.key}`;
        add(childPath, member.value.start);
        walk(member.value, childPath);
      }
      return;
    }
    if (node.kind === 'array' && node.items) {
      node.items.forEach((item, position) => {
        const indexed = `${path}[${position}]`;
        add(indexed, item.start);
        const name = stringMember(text, item, 'name');
        const id = stringMember(text, item, 'id');
        const aliases = [indexed];
        if (name !== null && name !== '') aliases.push(`${path}.${name}`);
        if (id !== null && id !== '') aliases.push(`${path}.${id}`);
        for (const alias of aliases) {
          if (alias !== indexed) add(alias, item.start);
          walk(item, alias);
        }
        const value = stringValue(text, item);
        if (value !== null && value !== '') add(`${path}.${value}`, item.start);
      });
    }
  };

  walk(root, '');
  return index;
}

/** Offsets of the start of every line, for offset → line/column. */
function lineStarts(text: string): number[] {
  const starts = [0];
  for (let offset = 0; offset < text.length; offset += 1) {
    if (text[offset] === '\n') starts.push(offset + 1);
  }
  return starts;
}

/** The 1-based line for a source offset. */
function positionAt(starts: readonly number[], offset: number): FilePosition {
  let low = 0;
  let high = starts.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if ((starts[middle] ?? 0) <= offset) low = middle;
    else high = middle - 1;
  }
  return { line: low + 1, column: offset - (starts[low] ?? 0) + 1 };
}

/** One line of text, 1-based, without its line ending. */
export function lineText(text: string, line: number): string {
  const lines = text.split(/\r?\n/);
  return lines[line - 1] ?? '';
}

/**
 * Strip the last segment of a field path.
 *
 * Handles both dotted names and trailing `[index]`, so
 * `workflows.build.steps[2].agent` walks back to `workflows.build.steps[2]`
 * and then `workflows.build.steps`.
 */
function withoutLastSegment(field: string): string | null {
  const bracket = /\[\d+\]$/.exec(field);
  if (bracket) return field.slice(0, bracket.index);
  const dot = field.lastIndexOf('.');
  if (dot <= 0) return null;
  return field.slice(0, dot);
}

/**
 * The source offset a field path resolves to, and whether it resolved at all.
 *
 * An exact node wins. Otherwise the path is shortened segment by segment until
 * an ancestor exists, so a missing required property points at the object that
 * should have held it. A path that never matches is `located: false`.
 */
function resolveFieldOffset(
  index: ReadonlyMap<string, number>,
  field: string,
): { offset: number; located: boolean } {
  let candidate = field;
  while (candidate !== '') {
    const exact = index.get(candidate);
    if (exact !== undefined) return { offset: exact, located: true };
    const shortened = withoutLastSegment(candidate);
    if (shortened === null) break;
    candidate = shortened;
  }
  const root = index.get('') ?? 0;
  return { offset: root, located: false };
}

/**
 * Locate one field path in an authored manifest.
 *
 * The line is the value's own line where the path exists, the nearest object's
 * line when a required property is missing, and line 1 with `located: false`
 * when the manifest is malformed or the path is unknown.
 */
export function locateField(manifest: string, field: string): FieldLocation {
  const root = parseJsonDocument(manifest);
  if (!root) {
    return { line: 1, column: 1, located: false, snippet: lineText(manifest, 1).trim() };
  }
  const index = indexDocument(manifest, root);
  const { offset, located } = resolveFieldOffset(index, field);
  const position = positionAt(lineStarts(manifest), offset);
  return {
    ...position,
    located,
    snippet: lineText(manifest, position.line).trim(),
  };
}

/**
 * Resolve every pool field issue against the authored bundle.
 *
 * Issues are returned in the pool's order, each with its file, line and the
 * text of that line. The manifest is the only file this contract's validator
 * names fields in; if a bundle has no manifest every issue is reported against
 * line 1 of the expected path with `located: false`.
 */
export function locateIssues(
  issues: readonly FieldIssue[],
  bundle: Bundle,
): LocatedIssue[] {
  const manifest = findManifest(bundle);
  return issues.map((issue) => {
    const location = manifest
      ? locateField(manifest.content, issue.field)
      : { line: 1, column: 1, located: false, snippet: '' };
    return {
      field: issue.field,
      message: issue.message,
      revision: issue.revision,
      file: MANIFEST_FILE,
      ...location,
    };
  });
}

/** Issues ordered by line, then by column, so the editor lists them top-down. */
export function sortLocatedIssues(issues: readonly LocatedIssue[]): LocatedIssue[] {
  return [...issues].sort(
    (left, right) =>
      left.line - right.line ||
      left.column - right.column ||
      left.field.localeCompare(right.field),
  );
}

/** The lines of a file that carry at least one issue. */
export function issueLines(issues: readonly LocatedIssue[]): number[] {
  return [...new Set(issues.map((issue) => issue.line))].sort((left, right) => left - right);
}

// ── diffs over authored files ─────────────────────────────────────────────

/** What happened to one authored file between two revisions. */
export type FileDiffStatus = 'added' | 'removed' | 'changed' | 'unchanged';

/** One line of a file diff. */
export interface DiffLine {
  /** Whether the line is only on the old side, only on the new side, or both. */
  kind: 'context' | 'added' | 'removed';
  /** The line's exact text. */
  text: string;
  /** Old-side line number, or null for an added line. */
  oldLine: number | null;
  /** New-side line number, or null for a removed line. */
  newLine: number | null;
}

/** One authored file in a revision diff. */
export interface FileDiff {
  /** The authored path. */
  path: string;
  /** What happened to the file. */
  status: FileDiffStatus;
  /** The aligned lines; empty for added/removed/unchanged whole files. */
  lines: DiffLine[];
  /** True when the file was too large to align and the diff summarizes it. */
  truncated: boolean;
  /** Old-side line count. */
  oldLines: number;
  /** New-side line count. */
  newLines: number;
}

/** A revision, named the way the history names it. */
export interface RevisionReference {
  /** Immutable revision id. */
  id: string;
  /** Per-project version number. */
  version: number;
}

/** One diff between two revisions. */
export interface RevisionComparison {
  /** The base revision, or null when there was no active revision. */
  from: RevisionReference | null;
  /** The revision being compared. */
  to: RevisionReference;
  /** True when both sides are the same revision. */
  identical: boolean;
  /** Files in authored order: base order first, then newly added files. */
  files: FileDiff[];
}

/** Split authored text into lines without inventing a trailing empty line. */
export function splitLines(content: string): string[] {
  if (content === '') return [];
  const lines = content.split('\n');
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/** Line-level alignment through a longest-common-subsequence table. */
function alignLines(before: readonly string[], after: readonly string[]): DiffLine[] {
  const rows = before.length + 1;
  const columns = after.length + 1;
  const table = new Uint32Array(rows * columns);
  const at = (row: number, column: number): number => table[row * columns + column] ?? 0;

  for (let row = before.length - 1; row >= 0; row -= 1) {
    for (let column = after.length - 1; column >= 0; column -= 1) {
      table[row * columns + column] =
        before[row] === after[column]
          ? at(row + 1, column + 1) + 1
          : Math.max(at(row + 1, column), at(row, column + 1));
    }
  }

  const lines: DiffLine[] = [];
  let row = 0;
  let column = 0;
  while (row < before.length && column < after.length) {
    if (before[row] === after[column]) {
      lines.push({
        kind: 'context',
        text: before[row] ?? '',
        oldLine: row + 1,
        newLine: column + 1,
      });
      row += 1;
      column += 1;
    } else if (at(row + 1, column) >= at(row, column + 1)) {
      lines.push({
        kind: 'removed',
        text: before[row] ?? '',
        oldLine: row + 1,
        newLine: null,
      });
      row += 1;
    } else {
      lines.push({
        kind: 'added',
        text: after[column] ?? '',
        oldLine: null,
        newLine: column + 1,
      });
      column += 1;
    }
  }
  while (row < before.length) {
    lines.push({
      kind: 'removed',
      text: before[row] ?? '',
      oldLine: row + 1,
      newLine: null,
    });
    row += 1;
  }
  while (column < after.length) {
    lines.push({
      kind: 'added',
      text: after[column] ?? '',
      oldLine: null,
      newLine: column + 1,
    });
    column += 1;
  }
  return lines;
}

/** One authored file compared across revisions. */
function diffFile(path: string, before: string | null, after: string | null): FileDiff {
  const beforeLines = before === null ? [] : splitLines(before);
  const afterLines = after === null ? [] : splitLines(after);
  const oldLines = beforeLines.length;
  const newLines = afterLines.length;
  if (before === null) {
    return {
      path,
      status: 'added',
      lines: afterLines.map((text, index) => ({
        kind: 'added' as const,
        text,
        oldLine: null,
        newLine: index + 1,
      })),
      truncated: false,
      oldLines,
      newLines,
    };
  }
  if (after === null) {
    return {
      path,
      status: 'removed',
      lines: beforeLines.map((text, index) => ({
        kind: 'removed' as const,
        text,
        oldLine: index + 1,
        newLine: null,
      })),
      truncated: false,
      oldLines,
      newLines,
    };
  }
  if (before === after) {
    return {
      path,
      status: 'unchanged',
      lines: [],
      truncated: false,
      oldLines,
      newLines,
    };
  }
  if (oldLines > MAX_DIFF_LINES || newLines > MAX_DIFF_LINES) {
    return {
      path,
      status: 'changed',
      lines: [],
      truncated: true,
      oldLines,
      newLines,
    };
  }
  return {
    path,
    status: 'changed',
    lines: alignLines(beforeLines, afterLines),
    truncated: false,
    oldLines,
    newLines,
  };
}

/**
 * Compare two authored bundles.
 *
 * Base order is kept; files only present in `after` follow in their own
 * authored order. A missing side means the file was added or removed.
 */
export function diffBundles(before: Bundle | null, after: Bundle | null): FileDiff[] {
  const oldFiles = new Map((before?.files ?? []).map((file) => [file.path, file.content]));
  const newFiles = new Map((after?.files ?? []).map((file) => [file.path, file.content]));
  const paths = [
    ...(before?.files ?? []).map((file) => file.path),
    ...(after?.files ?? [])
      .map((file) => file.path)
      .filter((path) => !oldFiles.has(path)),
  ];
  return paths.map((path) =>
    diffFile(path, oldFiles.get(path) ?? null, newFiles.get(path) ?? null),
  );
}

/** File diffs that actually differ, for a summary that does not list noise. */
export function changedFileDiffs(files: readonly FileDiff[]): FileDiff[] {
  return files.filter((file) => file.status !== 'unchanged');
}

/** A one-line summary of a revision diff. */
export function diffSummary(files: readonly FileDiff[]): string {
  const added = files.filter((file) => file.status === 'added').length;
  const removed = files.filter((file) => file.status === 'removed').length;
  const changed = files.filter((file) => file.status === 'changed').length;
  if (added + removed + changed === 0) return 'The two revisions hold identical authored files.';
  const parts: string[] = [];
  if (changed > 0) parts.push(`${changed} changed`);
  if (added > 0) parts.push(`${added} added`);
  if (removed > 0) parts.push(`${removed} removed`);
  return `${parts.join(', ')} ${parts.length === 1 ? 'file' : 'files'}.`;
}

// ── revision history ──────────────────────────────────────────────────────

/** One revision in the history list, in the form the page renders. */
export interface RevisionRow {
  /** Immutable revision id. */
  id: string;
  /** Per-project version number. */
  version: number;
  /** `valid` or `invalid`. */
  status: string;
  /** True when the pool marks this revision active. */
  active: boolean;
  /** How many named field issues the revision carries. */
  errorCount: number;
  /** When the revision was stored. */
  createdAt: string;
}

/** The active revision, and any reason it cannot be named unambiguously. */
export interface ActiveRevisionResolution {
  /** The active revision metadata, when exactly one is marked active. */
  active: RevisionMetaView | null;
  /** A readable reason the active revision is not unambiguous, or null. */
  problem: string | null;
}

/**
 * Resolve which revision is active from the project's history.
 *
 * Exactly one active revision is the only unambiguous answer. Zero when a
 * history exists, or more than one, is a problem with the pool's state that the
 * editor must show rather than paper over by picking the newest.
 */
export function resolveActiveRevision(
  revisions: readonly RevisionMetaView[],
): ActiveRevisionResolution {
  if (revisions.length === 0) return { active: null, problem: null };
  const active = revisions.filter((revision) => revision.active);
  if (active.length === 1) return { active: active[0] ?? null, problem: null };
  if (active.length === 0) {
    return {
      active: null,
      problem:
        'No revision is marked active, so there is no unambiguous configuration in force. The pool should mark exactly one revision active.',
    };
  }
  const versions = active
    .map((revision) => `v${revision.version}`)
    .sort((left, right) => left.localeCompare(right, undefined, { numeric: true }));
  return {
    active: null,
    problem: `The pool marks ${active.length} revisions active (${versions.join(', ')}), so the configuration in force is ambiguous.`,
  };
}

/** History rows newest version first, with the active flag carried through. */
export function revisionRows(revisions: readonly RevisionMetaView[]): RevisionRow[] {
  return [...revisions]
    .sort((left, right) => right.version - left.version)
    .map((revision) => ({
      id: revision.id,
      version: revision.version,
      status: revision.status,
      active: revision.active,
      errorCount: revision.error_count,
      createdAt: revision.created_at,
    }));
}

// ── the pool calls and the page controller ────────────────────────────────

/** The project and configuration calls the editor makes, injectable for tests. */
export interface ProjectsTransport {
  /** Every project in the organization. */
  list(): Promise<ProjectsResponse>;
  /** The active revision, or null when the project has none yet. */
  activeRevision(projectId: string): Promise<RevisionView | null>;
  /** The revision history, newest first. */
  revisions(projectId: string): Promise<RevisionsResponse>;
  /** One revision with its authored files. */
  revision(projectId: string, revisionId: string): Promise<RevisionView>;
  /** Dry-run validation through the pool's endpoint. */
  validate(projectId: string, body: ConfigurationRequest): Promise<ValidateResponse>;
  /** Install a validated bundle as a new immutable revision. */
  install(projectId: string, body: ConfigurationRequest): Promise<InstallResponse>;
}

/** Build the transport from the typed SDK client. */
export function projectsTransport(client: TidepoolClient): ProjectsTransport {
  return {
    list: () => client.api.projects.list(),
    activeRevision: async (projectId) => {
      try {
        return await client.api.projects.configuration(projectId);
      } catch (error) {
        // A project with no active configuration answers 404; that is a state,
        // not a failure of the page.
        if (isProblemError(error) && error.status === 404) return null;
        throw error;
      }
    },
    revisions: (projectId) => client.api.projects.revisions(projectId),
    revision: (projectId, revisionId) =>
      client.api.projects.revision({ id: projectId, revision_id: revisionId }),
    validate: (projectId, body) => client.api.projects.validateConfiguration(projectId, body),
    install: (projectId, body) => client.api.projects.installConfiguration(projectId, body),
  };
}

/** What the projects page is doing. */
export type ProjectsPhase = 'idle' | 'loading' | 'ready' | 'error';

/** Where a save stands. */
export type SavePhase = 'idle' | 'validating' | 'installing' | 'saved' | 'refused' | 'failed';

/** Everything the projects page renders. */
export interface ProjectsViewState {
  /** The current project-list load state. */
  phase: ProjectsPhase;
  /** Every project in the organization. */
  projects: ProjectView[];
  /** The project whose bundle is open, if any. */
  selectedProjectId: string | null;
  /** The draft bundle; authored order, exact content. */
  draft: Bundle;
  /** The active revision served by the pool, if any. */
  activeRevision: RevisionView | null;
  /** The active revision's id, when it is unambiguous. */
  activeRevisionId: string | null;
  /** A reason the active revision is ambiguous, or null. */
  activeProblem: string | null;
  /** The revision history, newest version first. */
  history: RevisionRow[];
  /** Named field issues, located against the draft's manifest. */
  issues: LocatedIssue[];
  /** Where the last save stands. */
  savePhase: SavePhase;
  /** The revision the last successful save activated. */
  savedRevision: RevisionView | null;
  /** The version a refused bundle would have become. */
  refusedVersion: number | null;
  /** A readable failure from the last save, if the call itself failed. */
  saveProblem: PoolProblem | null;
  /** The diff currently shown, if any. */
  comparison: RevisionComparison | null;
  /** The revision chosen for comparison. */
  selectedRevisionId: string | null;
  /** A failure loading projects, revisions or a diff. */
  problem: PoolProblem | null;
  /** True while a pool call is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyProjectsState(): ProjectsViewState {
  return {
    phase: 'idle',
    projects: [],
    selectedProjectId: null,
    draft: emptyBundle(),
    activeRevision: null,
    activeRevisionId: null,
    activeProblem: null,
    history: [],
    issues: [],
    savePhase: 'idle',
    savedRevision: null,
    refusedVersion: null,
    saveProblem: null,
    comparison: null,
    selectedRevisionId: null,
    problem: null,
    busy: false,
  };
}

/** Options for the projects controller. */
export interface ProjectsControllerOptions {
  /** The pool calls. */
  transport: ProjectsTransport;
}

/**
 * The projects page's controller.
 *
 * Every method returns the new state rather than mutating what the caller
 * holds, so a Svelte page can assign the result and a test can walk the whole
 * save/refuse/fix flow without a DOM. Nothing throws across the boundary:
 * pool failures become problem details in the state.
 *
 * A save is validate-then-install. A bundle that does not validate is never
 * installed, so the active revision cannot move. The draft is kept on refusal
 * with the issues attached, so the author fixes the line rather than retyping.
 */
export class ProjectsController {
  private readonly transport: ProjectsTransport;
  private state: ProjectsViewState = emptyProjectsState();

  constructor(options: ProjectsControllerOptions) {
    this.transport = options.transport;
  }

  /** The current state, without touching the pool. */
  snapshot(): ProjectsViewState {
    return this.state;
  }

  /** Load the project list, opening the first project. */
  async load(): Promise<ProjectsViewState> {
    this.state = { ...this.state, phase: this.state.phase === 'ready' ? 'ready' : 'loading' };
    try {
      const response = await this.transport.list();
      const projects = [...response.projects].sort((left, right) =>
        left.slug.localeCompare(right.slug),
      );
      this.state = {
        ...this.state,
        phase: 'ready',
        projects,
        problem: null,
      };
      const first = projects[0];
      if (first) return this.openProject(first.id);
    } catch (error) {
      this.state = { ...this.state, phase: 'error', problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Open one project's active revision and history. */
  async selectProject(projectId: string): Promise<ProjectsViewState> {
    return this.openProject(projectId);
  }

  /**
   * Replace one file's content.
   *
   * Only that file changes; order and every other file's bytes are kept. The
   * issues from a previous save are cleared because they described an older
   * draft.
   */
  editFile(path: string, content: string): ProjectsViewState {
    const draft = editBundleFile(this.state.draft, path, content);
    this.state = {
      ...this.state,
      draft,
      issues: [],
      savePhase: this.state.savePhase === 'saved' ? 'idle' : this.state.savePhase,
      savedRevision: null,
      refusedVersion: null,
    };
    return this.state;
  }

  /** Add a file at the end of the bundle; an existing path is untouched. */
  addFile(path: string, content = ''): ProjectsViewState {
    this.state = {
      ...this.state,
      draft: addBundleFile(this.state.draft, path, content),
      issues: [],
    };
    return this.state;
  }

  /** Remove one file; every other file keeps its order and content. */
  removeFile(path: string): ProjectsViewState {
    this.state = {
      ...this.state,
      draft: removeBundleFile(this.state.draft, path),
      issues: [],
    };
    return this.state;
  }

  /**
   * Save the draft: validate through the pool, install only if it validated.
   *
   * A refusal leaves the active revision exactly where it was and keeps the
   * draft, with every field issue located against the authored line. A call
   * failure is a different state (`failed`) and also leaves the active revision
   * alone.
   */
  async save(): Promise<ProjectsViewState> {
    const projectId = this.state.selectedProjectId;
    if (!projectId || this.state.busy) return this.state;
    if (this.state.activeProblem) {
      this.state = {
        ...this.state,
        savePhase: 'refused',
        saveProblem: {
          kind: 'app',
          title: 'The active revision is ambiguous',
          message: this.state.activeProblem,
          requestId: null,
          status: null,
          code: 'ambiguous_active_revision',
          type: null,
          retryAfterMs: null,
        },
      };
      return this.state;
    }

    // Locate errors against the exact draft that was sent, even if an edit
    // lands while the validation call is in flight.
    const sentBundle = this.state.draft;
    const request = requestFromBundle(sentBundle);
    this.state = {
      ...this.state,
      busy: true,
      savePhase: 'validating',
      saveProblem: null,
      issues: [],
      savedRevision: null,
      refusedVersion: null,
    };

    try {
      const validation = await this.transport.validate(projectId, request);
      if (!validation.valid) {
        this.state = {
          ...this.state,
          busy: false,
          savePhase: 'refused',
          issues: locateIssues(validation.errors, sentBundle),
          refusedVersion: validation.version,
        };
        return this.state;
      }

      this.state = { ...this.state, savePhase: 'installing' };
      const installed = await this.transport.install(projectId, request);
      if (!installed.activated) {
        const { rows } = await this.fetchHistory(projectId);
        this.state = {
          ...this.state,
          busy: false,
          savePhase: 'refused',
          issues: locateIssues(installed.revision.errors, bundleFromRevision(installed.revision)),
          refusedVersion: installed.revision.version,
          history: rows,
          saveProblem: {
            kind: 'app',
            title: 'The pool did not activate that revision',
            message: `The pool stored revision v${installed.revision.version} but did not activate it. The active revision is unchanged.`,
            requestId: null,
            status: null,
            code: 'revision_not_activated',
            type: null,
            retryAfterMs: null,
          },
        };
        return this.state;
      }

      const { rows, revisions } = await this.fetchHistory(projectId);
      const resolution = resolveActiveRevision(revisions);
      this.state = {
        ...this.state,
        busy: false,
        savePhase: 'saved',
        savedRevision: installed.revision,
        activeRevision: installed.revision,
        activeRevisionId: resolution.problem ? null : installed.revision.id,
        activeProblem: resolution.problem,
        draft: bundleFromRevision(installed.revision),
        issues: [],
        refusedVersion: null,
        history: rows,
        comparison: null,
        selectedRevisionId: null,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        savePhase: 'failed',
        saveProblem: presentProblem(error),
      };
    }
    return this.state;
  }

  /**
   * Compare one revision against another.
   *
   * With no `againstRevisionId` the base is the active revision, which is what
   * the history's "Compare with active" control asks for. Passing an explicit
   * base compares any two revisions. Both sides are authored files. If there is
   * no active revision the whole selected revision reads as added; comparing a
   * revision with itself is reported as identical rather than hidden.
   */
  async compare(
    revisionId: string,
    againstRevisionId?: string | null,
  ): Promise<ProjectsViewState> {
    const projectId = this.state.selectedProjectId;
    if (!projectId || this.state.busy) return this.state;
    this.state = { ...this.state, busy: true, problem: null };
    try {
      const target = await this.transport.revision(projectId, revisionId);
      let base = this.state.activeRevision;
      if (againstRevisionId) {
        base =
          base && base.id === againstRevisionId
            ? base
            : await this.transport.revision(projectId, againstRevisionId);
      }
      this.state = {
        ...this.state,
        busy: false,
        selectedRevisionId: revisionId,
        comparison: {
          from: base ? { id: base.id, version: base.version } : null,
          to: { id: target.id, version: target.version },
          identical: base !== null && base.id === target.id,
          files: diffBundles(
            base ? bundleFromRevision(base) : null,
            bundleFromRevision(target),
          ),
        },
      };
    } catch (error) {
      this.state = { ...this.state, busy: false, problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Close the diff view. */
  clearComparison(): ProjectsViewState {
    this.state = { ...this.state, comparison: null, selectedRevisionId: null };
    return this.state;
  }

  /** Forget a load failure, e.g. on retry. */
  clearProblem(): ProjectsViewState {
    this.state = { ...this.state, problem: null, saveProblem: null };
    return this.state;
  }

  /** Fetch the history once, keeping both the raw metadata and its rows. */
  private async fetchHistory(
    projectId: string,
  ): Promise<{ rows: RevisionRow[]; revisions: RevisionMetaView[] }> {
    const response = await this.transport.revisions(projectId);
    return { rows: revisionRows(response.revisions), revisions: response.revisions };
  }

  /** Open one project, fetching its active revision and history together. */
  private async openProject(projectId: string): Promise<ProjectsViewState> {
    this.state = {
      ...this.state,
      selectedProjectId: projectId,
      busy: true,
      problem: null,
      issues: [],
      comparison: null,
      selectedRevisionId: null,
      savePhase: 'idle',
      saveProblem: null,
      savedRevision: null,
      refusedVersion: null,
    };
    try {
      const active = await this.transport.activeRevision(projectId);
      const { rows, revisions } = await this.fetchHistory(projectId);
      const resolution = resolveActiveRevision(revisions);
      let activeProblem = resolution.problem;
      if (!activeProblem && resolution.active && active && resolution.active.id !== active.id) {
        activeProblem = `The pool serves revision v${active.version} as active, but its history marks v${resolution.active.version} active. The editor will not guess which is in force.`;
      } else if (!activeProblem && !resolution.active && active) {
        activeProblem = `The pool serves revision v${active.version} as active, but no revision in the history is marked active.`;
      } else if (!activeProblem && resolution.active && !active) {
        activeProblem = `The history marks revision v${resolution.active.version} active, but the pool serves no active configuration.`;
      }
      this.state = {
        ...this.state,
        busy: false,
        phase: 'ready',
        draft: active ? bundleFromRevision(active) : emptyBundle(),
        activeRevision: active,
        activeRevisionId: active?.id ?? null,
        activeProblem,
        history: rows,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        phase: 'error',
        problem: presentProblem(error),
      };
    }
    return this.state;
  }
}

/** The sentence beside the save control, from the last save's outcome. */
export function saveSummary(
  state: Pick<ProjectsViewState, 'savePhase' | 'savedRevision' | 'refusedVersion' | 'activeRevision'>,
): string {
  switch (state.savePhase) {
    case 'validating':
      return 'Validating the bundle with the pool…';
    case 'installing':
      return 'Installing the validated bundle as a new revision…';
    case 'saved':
      return state.savedRevision
        ? `Saved revision v${state.savedRevision.version} and activated it.`
        : 'The bundle was saved and activated.';
    case 'refused':
      return state.activeRevision
        ? `The pool refused this bundle. Revision v${state.activeRevision.version} remains active.`
        : 'The pool refused this bundle. No revision is active.';
    case 'failed':
      return 'The save did not complete. The active revision is unchanged.';
    case 'idle':
      return 'No unsaved changes have been sent yet.';
  }
}

/** The line naming which revision is in force, without guessing. */
export function activeRevisionLabel(
  state: Pick<ProjectsViewState, 'activeRevision' | 'activeProblem'>,
): string {
  if (state.activeProblem) return 'The active revision is ambiguous';
  if (!state.activeRevision) return 'No revision is active yet';
  return `Active revision v${state.activeRevision.version}`;
}

/** True when the page has both a pool and a credential to load projects with. */
export function canLoadProjects(
  config: PoolConfig,
  token: string | null | undefined,
): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}
