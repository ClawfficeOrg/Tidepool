/**
 * The trigger editor's model.
 *
 * A trigger is one authored document — `name`, `event`, `filters` and `run` —
 * and the editor can show it as a form or as YAML. This module owns that
 * document, the YAML projection and the host-checked agent choices, so every
 * rule can be tested without a DOM, without a live pool and without MySQL.
 *
 * # One document, two views
 *
 * The document is canonical. A form edit rewrites the document and the YAML
 * projection together; a YAML edit is parsed and only replaces the document
 * when the parse succeeds. While the YAML does not parse, the editor says so
 * and refuses to leave the YAML view, rather than showing a form that silently
 * describes something other than the text on screen.
 *
 * # The host is the authority on what it can run
 *
 * The pool stores each host's last capability report (`GET
 * /v1/hosts/{id}/capabilities`), and save-time validation asks the same
 * `host_capability::validate_agent_for_host` check the editor mirrors here.
 * [`agentOptionsFromReport`] narrows the report to the providers the pool may
 * offer; [`selectionAvailability`] compares the authored selection against
 * those options without ever rewriting it. A value the host no longer reports
 * is marked unavailable and kept, so the author decides what replaces it.
 *
 * # Saving is the pool's validation, twice
 *
 * [`preflightIssues`] catches the shape rules and the host/agent rules locally,
 * naming the same fields the pool names (`run.agent.model`), so a host that
 * cannot run the author's choice is refused before a request leaves the
 * browser. A preflight pass then calls `POST /v1/triggers/validate` and, only
 * when it is valid, `POST /v1/triggers/install`. Nothing is written on a
 * refusal.
 *
 * # The working directory
 *
 * The trigger contract names a host and an agent; it does not store a working
 * directory. The editor still asks for one, because the directory is the
 * context a host is asked about, and changing it reloads the agent choices.
 * It is deliberately not written into the YAML or the installed document: the
 * two views always describe the same trigger document, and inventing a field
 * the pool denies would make them describe different ones.
 */

import type {
  AgentSelection,
  HostCapabilityReport,
  HostView,
  HostsResponse,
  ProjectView,
  ProjectsResponse,
  TidepoolClient,
  TriggerDocument,
  TriggerInstallRequest,
  TriggerInstallResponse,
  TriggerIssue,
  TriggerValidateResponse,
} from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, type PoolConfig } from './api/pool';

/** Maximum characters in a trigger name (mirrors the pool). */
export const MAX_TRIGGER_NAME_CHARS = 255;
/** Maximum characters in an event type (mirrors the pool). */
export const MAX_EVENT_CHARS = 128;
/** Maximum characters in a resource kind (mirrors the pool). */
export const MAX_RESOURCE_KIND_CHARS = 64;
/** Maximum characters in a resource id (mirrors the pool). */
export const MAX_RESOURCE_ID_CHARS = 255;
/** Maximum characters in a channel or label guard (mirrors the pool). */
export const MAX_GUARD_CHARS = 255;
/** Maximum characters in a `contains` guard (mirrors the pool). */
export const MAX_CONTAINS_CHARS = 10_000;
/** Maximum characters in a `pattern` guard (mirrors the pool). */
export const MAX_PATTERN_CHARS = 1_000;
/** Maximum `from_users` entries (mirrors the pool). */
export const MAX_FROM_USERS = 10_000;
/** Maximum characters in one `from_users` handle (mirrors the pool). */
export const MAX_USER_CHARS = 255;

/** The providers a trigger event can name. */
export const EVENT_PROVIDERS = ['github', 'slack', 'discord'] as const;

/** One event provider. */
export type EventProvider = (typeof EVENT_PROVIDERS)[number];

/** The trigger document the editor starts from. */
export function emptyTriggerDocument(): TriggerDocument {
  return {
    name: '',
    event: { provider: 'github', type: '' },
    filters: {
      connection: '',
      resource: { kind: 'repository', id: '' },
      from_users: [],
      channel: null,
      label: null,
      contains: null,
      pattern: null,
    },
    run: {
      host: '',
      agent: {
        provider: '',
        model: '',
        execution_mode: '',
        thinking_option: null,
      },
    },
  };
}

/** Deep-copy a trigger document, preserving null optionals. */
export function cloneTriggerDocument(document: TriggerDocument): TriggerDocument {
  return {
    name: document.name,
    event: { provider: document.event.provider, type: document.event.type },
    filters: {
      connection: document.filters.connection,
      resource: {
        kind: document.filters.resource.kind,
        id: document.filters.resource.id,
      },
      from_users: [...document.filters.from_users],
      channel: document.filters.channel ?? null,
      label: document.filters.label ?? null,
      contains: document.filters.contains ?? null,
      pattern: document.filters.pattern ?? null,
    },
    run: {
      host: document.run.host,
      agent: {
        provider: document.run.agent.provider,
        model: document.run.agent.model,
        execution_mode: document.run.agent.execution_mode,
        thinking_option: document.run.agent.thinking_option ?? null,
      },
    },
  };
}

/** One form edit, typed so a component cannot send an unhandled path. */
export type TriggerEdit =
  | { field: 'filters.from_users'; value: string[] }
  | {
      field:
        | 'filters.channel'
        | 'filters.label'
        | 'filters.contains'
        | 'filters.pattern'
        | 'run.agent.thinking_option';
      value: string | null;
    }
  | {
      field:
        | 'name'
        | 'event.provider'
        | 'event.type'
        | 'filters.connection'
        | 'filters.resource.kind'
        | 'filters.resource.id'
        | 'run.host'
        | 'run.agent.provider'
        | 'run.agent.model'
        | 'run.agent.execution_mode';
      value: string;
    };

/** Apply one form edit to a document, returning a new document. */
export function applyTriggerEdit(document: TriggerDocument, edit: TriggerEdit): TriggerDocument {
  const next = cloneTriggerDocument(document);
  switch (edit.field) {
    case 'name':
      next.name = edit.value;
      break;
    case 'event.provider':
      next.event.provider = edit.value;
      break;
    case 'event.type':
      next.event.type = edit.value;
      break;
    case 'filters.connection':
      next.filters.connection = edit.value;
      break;
    case 'filters.resource.kind':
      next.filters.resource.kind = edit.value;
      break;
    case 'filters.resource.id':
      next.filters.resource.id = edit.value;
      break;
    case 'filters.from_users':
      next.filters.from_users = [...edit.value];
      break;
    case 'filters.channel':
      next.filters.channel = edit.value;
      break;
    case 'filters.label':
      next.filters.label = edit.value;
      break;
    case 'filters.contains':
      next.filters.contains = edit.value;
      break;
    case 'filters.pattern':
      next.filters.pattern = edit.value;
      break;
    case 'run.host':
      next.run.host = edit.value;
      break;
    case 'run.agent.provider':
      next.run.agent.provider = edit.value;
      break;
    case 'run.agent.model':
      next.run.agent.model = edit.value;
      break;
    case 'run.agent.execution_mode':
      next.run.agent.execution_mode = edit.value;
      break;
    case 'run.agent.thinking_option':
      next.run.agent.thinking_option = edit.value;
      break;
  }
  return next;
}

// ── YAML ───────────────────────────────────────────────────────────────────

/** A parsed YAML value. Scalars stay strings so ids are never coerced. */
type YamlValue = null | string | YamlValue[] | { [key: string]: YamlValue };

/** A YAML syntax failure with the line it was found on. */
class YamlSyntaxError extends Error {
  /** 1-based line number. */
  readonly line: number;

  constructor(line: number, message: string) {
    super(message);
    this.name = 'YamlSyntaxError';
    this.line = line;
  }
}

/** One significant YAML line. */
interface YamlLine {
  /** Leading space count. Tabs are refused during tokenisation. */
  indent: number;
  /** Content with indentation and comments removed. */
  text: string;
  /** 1-based source line. */
  number: number;
}

/** Strip a `#` comment, leaving quoted `#` characters alone. */
function stripYamlComment(line: string): string {
  let single = false;
  let double = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (character === '"' && !single && line[index - 1] !== '\\') {
      double = !double;
    } else if (character === "'" && !double) {
      single = !single;
    } else if (
      character === '#' &&
      !single &&
      !double &&
      (index === 0 || /\s/.test(line[index - 1] ?? ''))
    ) {
      return line.slice(0, index);
    }
  }
  return line;
}

/** Split the source into significant lines, refusing tab indentation. */
function tokeniseYaml(text: string): YamlLine[] {
  const lines: YamlLine[] = [];
  const raw = text.split(/\r\n|\r|\n/);
  raw.forEach((original, index) => {
    const number = index + 1;
    const withoutComment = stripYamlComment(original);
    if (withoutComment.trim() === '') return;
    const leading = /^[ \t]*/.exec(withoutComment)?.[0] ?? '';
    if (leading.includes('\t')) {
      throw new YamlSyntaxError(number, 'tabs are not allowed in indentation');
    }
    const indent = leading.length;
    const content = withoutComment.slice(indent).trimEnd();
    if (content === '---' || content === '...' || content.startsWith('%')) return;
    lines.push({ indent, text: content, number });
  });
  return lines;
}

/** Read one quoted scalar from the start of `raw`. */
function readQuoted(raw: string, line: number): { value: string; end: number } {
  const quote = raw[0];
  let value = '';
  let index = 1;
  while (index < raw.length) {
    const character = raw[index] ?? '';
    if (quote === '"') {
      if (character === '\\') {
        const escape = raw[index + 1] ?? '';
        const simple: Readonly<Record<string, string>> = {
          '"': '"',
          '\\': '\\',
          '/': '/',
          n: '\n',
          t: '\t',
          r: '\r',
          b: '\b',
          f: '\f',
        };
        if (escape === 'u') {
          const hex = raw.slice(index + 2, index + 6);
          if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
            throw new YamlSyntaxError(line, 'invalid \\u escape');
          }
          value += String.fromCharCode(Number.parseInt(hex, 16));
          index += 6;
          continue;
        }
        if (!(escape in simple)) {
          throw new YamlSyntaxError(line, `unknown escape \\${escape}`);
        }
        value += simple[escape] ?? '';
        index += 2;
        continue;
      }
      if (character === '"') return { value, end: index + 1 };
      value += character;
      index += 1;
      continue;
    }
    if (character === "'") {
      if (raw[index + 1] === "'") {
        value += "'";
        index += 2;
        continue;
      }
      return { value, end: index + 1 };
    }
    value += character;
    index += 1;
  }
  throw new YamlSyntaxError(line, 'unterminated quoted string');
}

/** Split the inside of a flow sequence on commas outside quotes. */
function splitFlowItems(inner: string, line: number): string[] {
  const items: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  for (let index = 0; index < inner.length; index += 1) {
    const character = inner[index] ?? '';
    if (quote !== null) {
      current += character;
      if (quote === '"' && character === '\\') {
        const next = inner[index + 1];
        if (next !== undefined) {
          current += next;
          index += 1;
        }
      } else if (character === quote) {
        if (quote === "'" && inner[index + 1] === "'") {
          current += "'";
          index += 1;
        } else {
          quote = null;
        }
      }
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      current += character;
    } else if (character === ',') {
      items.push(current.trim());
      current = '';
    } else {
      current += character;
    }
  }
  if (quote !== null) throw new YamlSyntaxError(line, 'unterminated quoted string in a list');
  items.push(current.trim());
  return items.filter((item) => item !== '');
}

/** Parse one scalar value. */
function parseScalar(raw: string, line: number): YamlValue {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  if (trimmed.startsWith('"') || trimmed.startsWith("'")) {
    const quoted = readQuoted(trimmed, line);
    if (trimmed.slice(quoted.end).trim() !== '') {
      throw new YamlSyntaxError(line, 'unexpected text after a quoted value');
    }
    return quoted.value;
  }
  if (trimmed === '~' || /^(null|Null|NULL)$/.test(trimmed)) return null;
  if (trimmed === '[]') return [];
  if (trimmed.startsWith('[')) {
    if (!trimmed.endsWith(']')) {
      throw new YamlSyntaxError(line, 'unterminated list');
    }
    return splitFlowItems(trimmed.slice(1, -1), line).map((item) => parseScalar(item, line));
  }
  if (trimmed.startsWith('{')) {
    throw new YamlSyntaxError(line, 'inline mappings are not supported');
  }
  if (trimmed.startsWith('|') || trimmed.startsWith('>')) {
    throw new YamlSyntaxError(line, 'block scalars are not supported');
  }
  return trimmed;
}

/** Find the `key: value` split outside quotes. */
function splitKey(text: string): { key: string; rest: string } | null {
  let single = false;
  let double = false;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (character === '"' && !single && text[index - 1] !== '\\') {
      double = !double;
    } else if (character === "'" && !double) {
      single = !single;
    } else if (
      character === ':' &&
      !single &&
      !double &&
      (index + 1 === text.length || /\s/.test(text[index + 1] ?? ''))
    ) {
      return { key: text.slice(0, index).trim(), rest: text.slice(index + 1) };
    }
  }
  return null;
}

/** Parse the plain keys this document uses: bare or quoted. */
function parseKey(raw: string, line: number): string {
  const trimmed = raw.trim();
  if (trimmed === '') throw new YamlSyntaxError(line, 'a mapping key is required');
  if (trimmed.startsWith('"') || trimmed.startsWith("'")) {
    const quoted = readQuoted(trimmed, line);
    if (trimmed.slice(quoted.end).trim() !== '') {
      throw new YamlSyntaxError(line, 'unexpected text after a key');
    }
    return quoted.value;
  }
  return trimmed;
}

/** A small block-YAML reader for the trigger document's shape. */
class YamlReader {
  private readonly lines: YamlLine[];
  private index = 0;

  constructor(lines: YamlLine[]) {
    this.lines = lines;
  }

  /** True once every significant line has been consumed. */
  get finished(): boolean {
    return this.index >= this.lines.length;
  }

  private current(): YamlLine {
    const line = this.lines[this.index];
    if (!line) throw new YamlSyntaxError(1, 'the document ended unexpectedly');
    return line;
  }

  /** Parse the root block. */
  parseRoot(): YamlValue {
    const first = this.current();
    const value = this.parseBlock(first.indent);
    if (!this.finished) {
      const leftover = this.current();
      throw new YamlSyntaxError(leftover.number, 'unexpected indentation');
    }
    return value;
  }

  private parseBlock(indent: number): YamlValue {
    const line = this.current();
    if (line.text === '-' || line.text.startsWith('- ')) return this.parseSequence(indent);
    return this.parseMapping(indent);
  }

  private parseMapping(indent: number): Record<string, YamlValue> {
    // A null prototype keeps a key such as `__proto__` an ordinary key rather
    // than changing the mapping's prototype.
    const mapping = Object.create(null) as Record<string, YamlValue>;
    while (!this.finished) {
      const line = this.current();
      if (line.indent < indent) break;
      if (line.indent > indent) {
        throw new YamlSyntaxError(line.number, 'unexpected indentation in a mapping');
      }
      if (line.text === '-' || line.text.startsWith('- ')) break;
      const split = splitKey(line.text);
      if (!split) {
        throw new YamlSyntaxError(line.number, 'expected "key: value"');
      }
      const key = parseKey(split.key, line.number);
      if (Object.prototype.hasOwnProperty.call(mapping, key)) {
        throw new YamlSyntaxError(line.number, `duplicate key "${key}"`);
      }
      this.index += 1;
      const rest = split.rest.trim();
      if (rest === '') {
        const next = this.lines[this.index];
        mapping[key] =
          next && next.indent > indent ? this.parseBlock(next.indent) : null;
      } else {
        mapping[key] = parseScalar(rest, line.number);
      }
    }
    return mapping;
  }

  private parseSequence(indent: number): YamlValue[] {
    const items: YamlValue[] = [];
    while (!this.finished) {
      const line = this.current();
      if (line.indent < indent) break;
      if (line.indent > indent) {
        throw new YamlSyntaxError(line.number, 'unexpected indentation in a list');
      }
      if (!(line.text === '-' || line.text.startsWith('- '))) break;
      const rest = line.text.slice(1).trimStart();
      this.index += 1;
      if (rest === '') {
        const next = this.lines[this.index];
        items.push(next && next.indent > indent ? this.parseBlock(next.indent) : null);
      } else {
        if (splitKey(rest) !== null) {
          throw new YamlSyntaxError(line.number, 'list entries must be scalar values');
        }
        items.push(parseScalar(rest, line.number));
      }
    }
    return items;
  }
}

/** Parse YAML text into a generic value, or report the syntax failure. */
function parseYamlText(text: string): YamlValue {
  const lines = tokeniseYaml(text);
  if (lines.length === 0) throw new YamlSyntaxError(1, 'the document is empty');
  return new YamlReader(lines).parseRoot();
}

/** The result of reading a trigger from YAML. */
export type TriggerYamlResult =
  | { ok: true; document: TriggerDocument }
  | { ok: false; message: string; line: number | null; issues: TriggerIssue[] };

/** True when a value is a YAML mapping. */
function isYamlMapping(value: YamlValue): value is { [key: string]: YamlValue } {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Flag keys the trigger contract does not define (it denies unknown fields). */
function checkYamlKeys(
  value: { [key: string]: YamlValue },
  allowed: readonly string[],
  where: string,
  issues: TriggerIssue[],
): void {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) {
      issues.push({ field: 'yaml', message: `unknown key "${key}" in ${where}` });
    }
  }
}

/** Read a required string field, reporting a wrong type. */
function yamlString(
  value: YamlValue,
  field: string,
  issues: TriggerIssue[],
): string | null {
  if (typeof value === 'string') return value;
  issues.push({ field, message: `${field} must be a string` });
  return null;
}

/** Read an optional string field: absent and null both mean "not set". */
function yamlOptionalString(
  value: YamlValue,
  field: string,
  issues: TriggerIssue[],
): string | null {
  if (value === null) return null;
  return yamlString(value, field, issues);
}

/** Read `from_users`, which must be a list of strings. */
function yamlUsers(value: YamlValue, issues: TriggerIssue[]): string[] {
  if (!Array.isArray(value)) {
    issues.push({ field: 'filters.from_users', message: 'filters.from_users must be a list' });
    return [];
  }
  const users: string[] = [];
  value.forEach((entry, index) => {
    if (typeof entry === 'string') {
      users.push(entry);
    } else {
      issues.push({
        field: `filters.from_users[${index}]`,
        message: `filters.from_users[${index}] must be a string`,
      });
    }
  });
  return users;
}

/** Build a document from a parsed YAML value, collecting shape issues. */
function documentFromYamlValue(value: YamlValue, issues: TriggerIssue[]): TriggerDocument {
  const document = emptyTriggerDocument();
  if (!isYamlMapping(value)) {
    issues.push({ field: 'yaml', message: 'the document must be a mapping of trigger fields' });
    return document;
  }
  checkYamlKeys(value, ['name', 'event', 'filters', 'run'], 'the trigger', issues);

  if ('name' in value) {
    document.name = yamlString(value['name'], 'name', issues) ?? '';
  }

  const event = value['event'];
  if (event !== undefined && event !== null) {
    if (!isYamlMapping(event)) {
      issues.push({ field: 'event', message: 'event must be a mapping' });
    } else {
      checkYamlKeys(event, ['provider', 'type'], 'event', issues);
      if ('provider' in event) {
        document.event.provider =
          yamlString(event['provider'], 'event.provider', issues) ?? '';
      }
      if ('type' in event) {
        document.event.type = yamlString(event['type'], 'event.type', issues) ?? '';
      }
    }
  } else if (event === null) {
    issues.push({ field: 'event', message: 'event must be a mapping' });
  }

  const filters = value['filters'];
  if (filters !== undefined && filters !== null) {
    if (!isYamlMapping(filters)) {
      issues.push({ field: 'filters', message: 'filters must be a mapping' });
    } else {
      checkYamlKeys(
        filters,
        [
          'connection',
          'resource',
          'from_users',
          'channel',
          'label',
          'contains',
          'pattern',
        ],
        'filters',
        issues,
      );
      if ('connection' in filters) {
        document.filters.connection =
          yamlString(filters['connection'], 'filters.connection', issues) ?? '';
      }
      const resource = filters['resource'];
      if (resource !== undefined && resource !== null) {
        if (!isYamlMapping(resource)) {
          issues.push({ field: 'filters.resource', message: 'filters.resource must be a mapping' });
        } else {
          checkYamlKeys(resource, ['kind', 'id'], 'filters.resource', issues);
          if ('kind' in resource) {
            document.filters.resource.kind =
              yamlString(resource['kind'], 'filters.resource.kind', issues) ?? '';
          }
          if ('id' in resource) {
            document.filters.resource.id =
              yamlString(resource['id'], 'filters.resource.id', issues) ?? '';
          }
        }
      } else if (resource === null) {
        issues.push({ field: 'filters.resource', message: 'filters.resource must be a mapping' });
      }
      if ('from_users' in filters) {
        document.filters.from_users = yamlUsers(filters['from_users'], issues);
      }
      for (const key of ['channel', 'label', 'contains', 'pattern'] as const) {
        if (key in filters) {
          const parsed = yamlOptionalString(
            filters[key] ?? null,
            `filters.${key}`,
            issues,
          );
          if (key === 'channel') document.filters.channel = parsed;
          if (key === 'label') document.filters.label = parsed;
          if (key === 'contains') document.filters.contains = parsed;
          if (key === 'pattern') document.filters.pattern = parsed;
        }
      }
    }
  } else if (filters === null) {
    issues.push({ field: 'filters', message: 'filters must be a mapping' });
  }

  const run = value['run'];
  if (run !== undefined && run !== null) {
    if (!isYamlMapping(run)) {
      issues.push({ field: 'run', message: 'run must be a mapping' });
    } else {
      checkYamlKeys(run, ['host', 'agent'], 'run', issues);
      if ('host' in run) {
        document.run.host = yamlString(run['host'], 'run.host', issues) ?? '';
      }
      const agent = run['agent'];
      if (agent !== undefined && agent !== null) {
        if (!isYamlMapping(agent)) {
          issues.push({ field: 'run.agent', message: 'run.agent must be a mapping' });
        } else {
          checkYamlKeys(
            agent,
            ['provider', 'model', 'execution_mode', 'thinking_option'],
            'run.agent',
            issues,
          );
          if ('provider' in agent) {
            document.run.agent.provider =
              yamlString(agent['provider'], 'run.agent.provider', issues) ?? '';
          }
          if ('model' in agent) {
            document.run.agent.model =
              yamlString(agent['model'], 'run.agent.model', issues) ?? '';
          }
          if ('execution_mode' in agent) {
            document.run.agent.execution_mode =
              yamlString(agent['execution_mode'], 'run.agent.execution_mode', issues) ?? '';
          }
          if ('thinking_option' in agent) {
            document.run.agent.thinking_option = yamlOptionalString(
              agent['thinking_option'],
              'run.agent.thinking_option',
              issues,
            );
          }
        }
      } else if (agent === null) {
        issues.push({ field: 'run.agent', message: 'run.agent must be a mapping' });
      }
    }
  } else if (run === null) {
    issues.push({ field: 'run', message: 'run must be a mapping' });
  }

  return document;
}

/**
 * Read a trigger document from YAML.
 *
 * Missing sections keep the editor's empty defaults so an author can build the
 * document incrementally; wrong types, unknown keys and syntax errors are
 * refusals, because accepting them would make the form describe a different
 * document than the text.
 */
export function documentFromYaml(text: string): TriggerYamlResult {
  let value: YamlValue;
  try {
    value = parseYamlText(text);
  } catch (error) {
    if (error instanceof YamlSyntaxError) {
      const message = `YAML line ${error.line}: ${error.message}`;
      return { ok: false, message, line: error.line, issues: [{ field: 'yaml', message }] };
    }
    const message = error instanceof Error ? error.message : 'the YAML could not be read';
    return { ok: false, message, line: null, issues: [{ field: 'yaml', message }] };
  }

  const issues: TriggerIssue[] = [];
  const document = documentFromYamlValue(value, issues);
  if (issues.length > 0) {
    return {
      ok: false,
      message: 'The YAML does not describe a trigger document.',
      line: null,
      issues,
    };
  }
  return { ok: true, document };
}

/** True when a plain scalar needs quoting to round-trip as a string. */
function yamlNeedsQuotes(value: string): boolean {
  if (value === '') return true;
  if (/^(null|~|true|false|yes|no|on|off)$/i.test(value)) return true;
  if (/^[\s]|[\s]$/.test(value)) return true;
  if (/[\n\r\t]/.test(value)) return true;
  if (/^[-?:,[\]{}#&*!|>'"%@`]/.test(value)) return true;
  if (/: | #/.test(value)) return true;
  return false;
}

/** Encode one scalar. */
function yamlScalar(value: string | null): string {
  if (value === null) return 'null';
  return yamlNeedsQuotes(value) ? JSON.stringify(value) : value;
}

/**
 * Encode a trigger document as YAML, in a fixed key order.
 *
 * The order is the reading order of the form: identity, event, filters, run.
 * Every optional guard is written as `null` when unset, so the written document
 * is complete and a reader never has to guess whether an absent key means "no
 * guard" or "not written yet".
 */
export function triggerYaml(document: TriggerDocument): string {
  const lines: string[] = [
    `name: ${yamlScalar(document.name)}`,
    'event:',
    `  provider: ${yamlScalar(document.event.provider)}`,
    `  type: ${yamlScalar(document.event.type)}`,
    'filters:',
    `  connection: ${yamlScalar(document.filters.connection)}`,
    '  resource:',
    `    kind: ${yamlScalar(document.filters.resource.kind)}`,
    `    id: ${yamlScalar(document.filters.resource.id)}`,
  ];
  if (document.filters.from_users.length === 0) {
    lines.push('  from_users: []');
  } else {
    lines.push('  from_users:');
    for (const user of document.filters.from_users) {
      lines.push(`    - ${yamlScalar(user)}`);
    }
  }
  lines.push(
    `  channel: ${yamlScalar(document.filters.channel ?? null)}`,
    `  label: ${yamlScalar(document.filters.label ?? null)}`,
    `  contains: ${yamlScalar(document.filters.contains ?? null)}`,
    `  pattern: ${yamlScalar(document.filters.pattern ?? null)}`,
    'run:',
    `  host: ${yamlScalar(document.run.host)}`,
    '  agent:',
    `    provider: ${yamlScalar(document.run.agent.provider)}`,
    `    model: ${yamlScalar(document.run.agent.model)}`,
    `    execution_mode: ${yamlScalar(document.run.agent.execution_mode)}`,
    `    thinking_option: ${yamlScalar(document.run.agent.thinking_option ?? null)}`,
  );
  return `${lines.join('\n')}\n`;
}

// ── hosts and agent choices ────────────────────────────────────────────────

/** One host, as the editor's chooser reads it. */
export interface TriggerHostRow {
  /** Immutable public id, used for the capability call. */
  id: string;
  /** Mutable slug, which the trigger document stores. */
  slug: string;
  /** Name a person sees. */
  displayName: string;
  /** Raw lifecycle status. */
  status: string;
  /** Whether `host.execute` is granted. */
  executionEnabled: boolean;
  /** True when the pool has no live connection. */
  offline: boolean;
  /** True when the host can be asked for and accept agent work. */
  runnable: boolean;
}

/** Build the editor's host rows, runnable hosts first and then by slug. */
export function triggerHostRows(hosts: readonly HostView[]): TriggerHostRow[] {
  const rows = hosts.map((host): TriggerHostRow => {
    const offline = host.status === 'offline';
    const revoked = host.status === 'revoked';
    const pending = host.status === 'pending_approval';
    return {
      id: host.id,
      slug: host.slug,
      displayName: host.display_name,
      status: host.status,
      executionEnabled: host.execution_enabled,
      offline,
      runnable: host.execution_enabled && !offline && !revoked && !pending,
    };
  });
  return rows.sort((left, right) => {
    if (left.runnable !== right.runnable) return left.runnable ? -1 : 1;
    const bySlug = left.slug.localeCompare(right.slug);
    if (bySlug !== 0) return bySlug;
    return left.id.localeCompare(right.id);
  });
}

/** The nested choices a capability report offers, keyed for the cascade. */
export interface AgentOptions {
  /** Providers the pool may offer, in sorted order. */
  providers: string[];
  /** Models by provider. */
  models: Readonly<Record<string, string[]>>;
  /** Execution modes by `provider\u0000model`. */
  modes: Readonly<Record<string, string[]>>;
  /** Non-null thinking options by `provider\u0000model\u0000mode`. */
  thinking: Readonly<Record<string, string[]>>;
}

/** The separator used in the cascade keys; NUL cannot occur in a provider id. */
const KEY_SEPARATOR = '\u0000';

/** Build the cascade key for a provider and model pair. */
export function agentVariantKey(provider: string, model: string): string {
  return `${provider}${KEY_SEPARATOR}${model}`;
}

/** Build the cascade key for a full provider/model/mode triple. */
export function agentThinkingKey(provider: string, model: string, mode: string): string {
  return `${provider}${KEY_SEPARATOR}${model}${KEY_SEPARATOR}${mode}`;
}

/** Sorted, de-duplicated strings. */
function uniqueSorted(values: readonly string[]): string[] {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}

/**
 * Narrow a capability report to the choices the editor offers.
 *
 * Only providers the pool lists in `offered_providers` are kept: a host may
 * report anything, but the pool refuses a trigger naming a provider outside
 * its grantable set, so offering it would set the author up for a refusal.
 */
export function agentOptionsFromReport(report: HostCapabilityReport): AgentOptions {
  const offered = new Set(report.offered_providers.filter((provider) => provider.trim() !== ''));
  const providers = uniqueSorted([...offered]);
  const models = new Map<string, Set<string>>();
  const modes = new Map<string, Set<string>>();
  const thinking = new Map<string, Set<string>>();

  for (const capability of report.capabilities) {
    if (!offered.has(capability.provider)) continue;
    const providerModels = models.get(capability.provider) ?? new Set<string>();
    providerModels.add(capability.model);
    models.set(capability.provider, providerModels);

    const variant = agentVariantKey(capability.provider, capability.model);
    const variantModes = modes.get(variant) ?? new Set<string>();
    variantModes.add(capability.execution_mode);
    modes.set(variant, variantModes);

    const thinkingOption = capability.thinking_option;
    if (typeof thinkingOption === 'string' && thinkingOption.trim() !== '') {
      const key = agentThinkingKey(
        capability.provider,
        capability.model,
        capability.execution_mode,
      );
      const options = thinking.get(key) ?? new Set<string>();
      options.add(thinkingOption);
      thinking.set(key, options);
    }
  }

  const record = (source: Map<string, Set<string>>): Record<string, string[]> => {
    const result: Record<string, string[]> = {};
    for (const [key, values] of source) {
      result[key] = uniqueSorted([...values]);
    }
    return result;
  };

  return {
    providers,
    models: record(models),
    modes: record(modes),
    thinking: record(thinking),
  };
}

/** One agent field's availability. */
export type AvailabilityState = 'available' | 'unavailable' | 'unset' | 'unknown';

/** One field in the agent selection. */
export interface AgentFieldAvailability {
  /** The contract's field name. */
  field: 'provider' | 'model' | 'execution_mode' | 'thinking_option';
  /** The authored value, or null for no thinking option. */
  value: string | null;
  /** Whether the value is offered, missing, or cannot be judged right now. */
  state: AvailabilityState;
}

/** The whole agent selection's availability. */
export interface SelectionAvailability {
  /** Provider, model, mode and thinking option, in field order. */
  fields: AgentFieldAvailability[];
  /** True when every authored value is offered by the host. */
  ok: boolean;
  /** The fields that are authored but not offered. */
  unavailable: AgentFieldAvailability[];
}

/**
 * Compare an authored selection with the host's options.
 *
 * A value is never changed here. With no options (no host chosen, loading, or
 * an offline host) an authored value is `unknown`, not `unavailable`: the
 * editor cannot claim the host cannot run it when it has not heard from the
 * host. Only a ready report can produce `unavailable`, which is the state the
 * page flags in red and refuses to save.
 */
export function selectionAvailability(
  options: AgentOptions | null,
  selection: AgentSelection,
): SelectionAvailability {
  const provider = selection.provider.trim();
  const model = selection.model.trim();
  const mode = selection.execution_mode.trim();
  const thinking = selection.thinking_option ?? null;

  const judge = (value: string | null, offered: readonly string[] | null): AvailabilityState => {
    if (value === null || value.trim() === '') return 'unset';
    if (offered === null) return 'unknown';
    return offered.includes(value) ? 'available' : 'unavailable';
  };

  const providerChoices = options?.providers ?? null;
  const modelChoices =
    options === null || provider === '' ? null : options.models[provider] ?? [];
  const modeChoices =
    options === null || provider === '' || model === ''
      ? null
      : options.modes[agentVariantKey(provider, model)] ?? [];
  const thinkingChoices =
    options === null || provider === '' || model === '' || mode === ''
      ? []
      : options.thinking[agentThinkingKey(provider, model, mode)] ?? [];

  const fields: AgentFieldAvailability[] = [
    { field: 'provider', value: selection.provider, state: judge(provider, providerChoices) },
    { field: 'model', value: selection.model, state: judge(model, modelChoices) },
    {
      field: 'execution_mode',
      value: selection.execution_mode,
      state: judge(mode, modeChoices),
    },
    {
      field: 'thinking_option',
      value: thinking,
      state:
        thinking === null || thinking.trim() === ''
          ? 'unset'
          : options === null
            ? 'unknown'
            : thinkingChoices.includes(thinking)
              ? 'available'
              : 'unavailable',
    },
  ];

  const unavailable = fields.filter((entry) => entry.state === 'unavailable');
  return { fields, ok: unavailable.length === 0, unavailable };
}

/** The `run.agent.*` field for one selection field. */
export function agentIssueField(field: AgentFieldAvailability['field']): string {
  return `run.agent.${field}`;
}

/**
 * Issues for authored agent values the host does not offer.
 *
 * The wording matches the pool's `resolve_issues`:
 * `host 'swaygent' cannot run 'claude'` at `run.agent.model`.
 */
export function unavailableAgentIssues(
  options: AgentOptions | null,
  selection: AgentSelection,
  hostSlug: string,
): TriggerIssue[] {
  if (options === null) return [];
  return selectionAvailability(options, selection).unavailable.map((entry) => ({
    field: agentIssueField(entry.field),
    message: `host '${hostSlug}' cannot run '${entry.value ?? ''}'`,
  }));
}

// ── the trigger model's own validation ─────────────────────────────────────

/** True when a value is one of the three event providers. */
export function isEventProvider(value: string): value is EventProvider {
  return (EVENT_PROVIDERS as readonly string[]).includes(value);
}

/**
 * Shape issues, mirroring the pool's `structural_issues`.
 *
 * The messages are the pool's own so a local refusal reads exactly like the
 * refusal the same document would get over HTTP; the tests compare the field
 * names, which are the contract.
 */
export function structuralIssues(document: TriggerDocument): TriggerIssue[] {
  const issues: TriggerIssue[] = [];
  const push = (field: string, message: string): void => {
    issues.push({ field, message });
  };

  if (document.name.trim() === '') {
    push('name', 'trigger name is required');
  } else if (document.name.length > MAX_TRIGGER_NAME_CHARS) {
    push('name', `trigger name must be at most ${MAX_TRIGGER_NAME_CHARS} characters`);
  }

  if (!isEventProvider(document.event.provider)) {
    push('event.provider', 'event provider must be github, slack or discord');
  }
  if (document.event.type.trim() === '') {
    push('event.type', 'event type is required');
  } else if (document.event.type.length > MAX_EVENT_CHARS) {
    push('event.type', `event type must be at most ${MAX_EVENT_CHARS} characters`);
  }

  if (document.filters.connection.trim() === '') {
    push('filters.connection', 'connection slug is required');
  }
  if (document.filters.from_users.length === 0) {
    push('filters.from_users', 'from_users must be present and non-empty');
  } else {
    if (document.filters.from_users.length > MAX_FROM_USERS) {
      push('filters.from_users', `from_users must have at most ${MAX_FROM_USERS} entries`);
    }
    document.filters.from_users.forEach((user, index) => {
      if (user.trim() === '') {
        push(`filters.from_users[${index}]`, 'user handle is required');
      } else if (user.length > MAX_USER_CHARS) {
        push(
          `filters.from_users[${index}]`,
          `user handle must be at most ${MAX_USER_CHARS} characters`,
        );
      }
    });
  }

  if (document.filters.resource.kind.trim() === '') {
    push('filters.resource.kind', 'resource kind is required');
  } else if (document.filters.resource.kind.length > MAX_RESOURCE_KIND_CHARS) {
    push(
      'filters.resource.kind',
      `resource kind must be at most ${MAX_RESOURCE_KIND_CHARS} characters`,
    );
  }
  if (document.filters.resource.id.trim() === '') {
    push('filters.resource.id', 'resource id is required');
  } else if (document.filters.resource.id.length > MAX_RESOURCE_ID_CHARS) {
    push(
      'filters.resource.id',
      `resource id must be at most ${MAX_RESOURCE_ID_CHARS} characters`,
    );
  }

  const guards: ReadonlyArray<[keyof typeof document.filters, string]> = [
    ['channel', 'channel'],
    ['label', 'label'],
    ['contains', 'contains'],
    ['pattern', 'pattern'],
  ];
  for (const [key, field] of guards) {
    const value = document.filters[key];
    if (typeof value !== 'string') continue;
    const limit =
      key === 'contains' ? MAX_CONTAINS_CHARS : key === 'pattern' ? MAX_PATTERN_CHARS : MAX_GUARD_CHARS;
    if (key === 'contains' || key === 'pattern') {
      if (value === '') push(`filters.${field}`, `${field} guard must be non-empty`);
      else if (value.length > limit) {
        push(`filters.${field}`, `${field} guard must be at most ${limit} characters`);
      }
    } else if (value.trim() === '') {
      push(`filters.${field}`, `${field} must be non-empty`);
    } else if (value.length > limit) {
      push(`filters.${field}`, `${field} must be at most ${limit} characters`);
    }
  }

  if (document.run.host.trim() === '') {
    push('run.host', 'host slug is required');
  }
  const agent = document.run.agent;
  if (agent.provider.trim() === '') push('run.agent.provider', 'provider is required');
  if (agent.model.trim() === '') push('run.agent.model', 'model is required');
  if (agent.execution_mode.trim() === '') {
    push('run.agent.execution_mode', 'execution mode is required');
  }
  if (
    agent.thinking_option !== null &&
    agent.thinking_option !== undefined &&
    agent.thinking_option.trim() === ''
  ) {
    push('run.agent.thinking_option', 'thinking option must be non-empty');
  }

  return issues;
}

/** The state of the host the editor asked for its agent choices. */
export type AgentChoicesState =
  | { status: 'idle' }
  | { status: 'loading'; host: string; directory: string }
  | {
      status: 'ready';
      host: string;
      directory: string;
      report: HostCapabilityReport;
      options: AgentOptions;
    }
  | {
      status: 'offline';
      host: string;
      directory: string;
      message: string;
      problem: PoolProblem | null;
    }
  | { status: 'unavailable' | 'unknown-host'; host: string; directory: string; message: string };

/** The issue text for a host with no execution grant. */
export function hostNotGrantedMessage(slug: string): string {
  return `host '${slug}' has not been granted execution`;
}

/** The explanation shown when a host is offline. */
export function offlineHostMessage(slug: string, directory: string): string {
  const where = directory.trim() === '' ? 'the selected directory' : `'${directory.trim()}'`;
  return `The host '${slug}' is offline, so the pool cannot ask it what it can run in ${where}. Start the host, then retry. The rest of the trigger stays editable.`;
}

/** The explanation shown when a host is revoked. */
export function revokedHostMessage(slug: string): string {
  return `The host '${slug}' is revoked; the pool refuses it everywhere, so it cannot run this trigger.`;
}

/** The explanation shown for a host that has never connected. */
export function pendingHostMessage(slug: string): string {
  return `The host '${slug}' has registered but has not connected yet, so it has no capability report.`;
}

/** Everything needed for the local save-time check. */
export interface PreflightInput {
  /** The authored document. */
  document: TriggerDocument;
  /** The hosts the pool listed. */
  hosts: readonly TriggerHostRow[];
  /** The last loaded agent choices. */
  choices: AgentChoicesState;
}

/**
 * The local half of save validation.
 *
 * Shape first, then the host reference, then the host's report when one is
 * loaded. The pool runs the same rules again (and the connection and resource
 * lookups this page cannot do); a document that fails here never reaches it.
 */
export function preflightIssues(input: PreflightInput): TriggerIssue[] {
  const issues = structuralIssues(input.document);
  const slug = input.document.run.host.trim();
  if (slug !== '') {
    const host = input.hosts.find((candidate) => candidate.slug === slug);
    if (!host) {
      issues.push({ field: 'run.host', message: `unknown host slug '${slug}'` });
    } else if (host.status === 'revoked') {
      issues.push({ field: 'run.host', message: `host '${slug}' is revoked` });
    } else if (!host.executionEnabled) {
      issues.push({ field: 'run.host', message: hostNotGrantedMessage(slug) });
    }
  }
  if (
    input.choices.status === 'ready' &&
    input.choices.host === slug &&
    slug !== ''
  ) {
    issues.push(
      ...unavailableAgentIssues(input.choices.options, input.document.run.agent, slug),
    );
  }
  return issues;
}

// ── the page's state and controller ───────────────────────────────────────

/** The pool calls the trigger editor makes, injectable for tests. */
export interface TriggersTransport {
  /** List the organization's projects, for the owning project chooser. */
  listProjects(): Promise<ProjectsResponse>;
  /** List live hosts, for the host chooser. */
  listHosts(): Promise<HostsResponse>;
  /**
   * Read a host's last capability report.
   *
   * The directory is part of the call so a pool that scopes capabilities by
   * working directory can use it; this contract revision ignores it, and the
   * editor still reloads when it changes.
   */
  capabilities(hostId: string, directory: string): Promise<HostCapabilityReport>;
  /** Validate one document without writing anything. */
  validate(document: TriggerDocument): Promise<TriggerValidateResponse>;
  /** Install or replace the project's trigger of that name. */
  install(request: TriggerInstallRequest): Promise<TriggerInstallResponse>;
}

/** Build the transport from the typed SDK client. */
export function triggersTransport(client: TidepoolClient): TriggersTransport {
  return {
    listProjects: () => client.api.projects.list(),
    listHosts: () => client.api.hosts.list(),
    capabilities: (hostId: string) => client.api.hosts.capabilities(hostId),
    validate: (document) => client.api.triggers.validate(document),
    install: (request) => client.api.triggers.install(request),
  };
}

/** Which view the author is editing. */
export type TriggerViewMode = 'form' | 'yaml';

/** Where a save is in its lifecycle. */
export type TriggerSavePhase = 'idle' | 'validating' | 'installing' | 'saved' | 'refused' | 'failed';

/** Everything the trigger page renders. */
export interface TriggersViewState {
  /** The current load state. */
  phase: 'idle' | 'loading' | 'ready' | 'error';
  /** Projects the trigger can belong to. */
  projects: ProjectView[];
  /** The chosen project id. */
  selectedProjectId: string | null;
  /** Hosts the pool listed, runnable first. */
  hosts: TriggerHostRow[];
  /** The canonical trigger document. */
  document: TriggerDocument;
  /** The working directory the editor asks the host about. Not installed. */
  directory: string;
  /** Which view is shown. */
  mode: TriggerViewMode;
  /** The YAML text, kept in step with the document or flagged when it is not. */
  yaml: string;
  /** A YAML parse failure, or null. */
  yamlError: string | null;
  /** The last loaded agent choices. */
  choices: AgentChoicesState;
  /** Named issues from a preflight, the pool, or a YAML parse. */
  issues: TriggerIssue[];
  /** A failure loading the page. */
  problem: PoolProblem | null;
  /** Where a save is. */
  savePhase: TriggerSavePhase;
  /** A failure from the pool while saving. */
  saveProblem: PoolProblem | null;
  /** The installed trigger, after a successful save. */
  saved: TriggerInstallResponse | null;
  /** True while a save or a capability call is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyTriggersState(): TriggersViewState {
  const document = emptyTriggerDocument();
  return {
    phase: 'idle',
    projects: [],
    selectedProjectId: null,
    hosts: [],
    document,
    directory: '',
    mode: 'form',
    yaml: triggerYaml(document),
    yamlError: null,
    choices: { status: 'idle' },
    issues: [],
    problem: null,
    savePhase: 'idle',
    saveProblem: null,
    saved: null,
    busy: false,
  };
}

/** The host row for a slug, or null. */
export function findHost(
  hosts: readonly TriggerHostRow[],
  slug: string,
): TriggerHostRow | null {
  const trimmed = slug.trim();
  if (trimmed === '') return null;
  return hosts.find((host) => host.slug === trimmed) ?? null;
}

/** A one-line summary of a save. */
export function saveSummary(state: TriggersViewState): string {
  switch (state.savePhase) {
    case 'validating':
      return 'Checking the trigger with the pool…';
    case 'installing':
      return 'Installing the trigger…';
    case 'saved':
      return state.saved
        ? `Installed '${state.saved.name}' version ${state.saved.version}${
            state.saved.replaced ? ' (replaced)' : ''
          }.`
        : 'Installed.';
    case 'refused':
      return state.issues.length === 0
        ? 'The pool refused the trigger; nothing was installed.'
        : `Nothing was installed: ${state.issues.length} field${
            state.issues.length === 1 ? '' : 's'
          } need attention.`;
    case 'failed':
      return 'The save did not reach the pool; nothing was installed.';
    default:
      return 'Not saved yet.';
  }
}

/** True when the trigger editor can load from this pool config. */
export function canLoadTriggers(
  config: PoolConfig,
  token: string | null | undefined,
): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}

/** How the controller reaches the pool. */
export interface TriggersControllerOptions {
  /** The pool calls. */
  transport: TriggersTransport;
  /** A resumed or preloaded state; tests use this to start at a fixture. */
  initial?: TriggersViewState;
}

/** The trigger editor's controller. */
export class TriggersController {
  private readonly transport: TriggersTransport;
  private state: TriggersViewState;
  /**
   * Guards against an older capability answer replacing a newer one when the
   * author changes host or directory twice quickly.
   */
  private choicesToken = 0;

  constructor(options: TriggersControllerOptions) {
    this.transport = options.transport;
    this.state = options.initial ?? emptyTriggersState();
  }

  /** The current state, without touching the pool. */
  snapshot(): TriggersViewState {
    return this.state;
  }

  /** Load projects and hosts, then the choices for an already-authored host. */
  async load(): Promise<TriggersViewState> {
    this.state = {
      ...this.state,
      phase: this.state.phase === 'ready' ? 'ready' : 'loading',
      problem: null,
    };
    try {
      const [projects, hosts] = await Promise.all([
        this.transport.listProjects(),
        this.transport.listHosts(),
      ]);
      const rows = triggerHostRows(hosts.hosts);
      const selectedProjectId =
        this.state.selectedProjectId ??
        projects.projects[0]?.id ??
        null;
      this.state = {
        ...this.state,
        phase: 'ready',
        projects: [...projects.projects],
        selectedProjectId,
        hosts: rows,
        problem: null,
      };
      if (this.state.document.run.host.trim() !== '') {
        await this.reloadChoices();
      }
    } catch (error) {
      this.state = { ...this.state, phase: 'error', problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Choose the project the trigger belongs to. */
  selectProject(projectId: string): TriggersViewState {
    this.state = { ...this.state, selectedProjectId: projectId };
    return this.state;
  }

  /** Switch between the form and the YAML view. */
  setMode(mode: TriggerViewMode): TriggersViewState {
    if (this.state.yamlError !== null) return this.state;
    if (mode === this.state.mode) return this.state;
    const yaml = mode === 'yaml' ? triggerYaml(this.state.document) : this.state.yaml;
    this.state = { ...this.state, mode, yaml };
    return this.state;
  }

  /** Apply one form edit and rewrite the YAML projection. */
  apply(edit: TriggerEdit): TriggersViewState {
    const document = applyTriggerEdit(this.state.document, edit);
    this.state = {
      ...this.state,
      document,
      yaml: triggerYaml(document),
      yamlError: null,
      issues: [],
      savePhase: 'idle',
      saveProblem: null,
      saved: null,
    };
    return this.state;
  }

  /** Choose the host and reload the agent choices for it. */
  async setHost(slug: string): Promise<TriggersViewState> {
    this.apply({ field: 'run.host', value: slug });
    return this.reloadChoices();
  }

  /** Change the working directory and reload the agent choices. */
  async setDirectory(directory: string): Promise<TriggersViewState> {
    const changed = directory !== this.state.directory;
    this.state = { ...this.state, directory };
    if (!changed) return this.state;
    return this.reloadChoices();
  }

  /** Read the YAML the author typed, adopting it only when it parses. */
  editYaml(text: string): TriggersViewState {
    const parsed = documentFromYaml(text);
    if (!parsed.ok) {
      this.state = {
        ...this.state,
        yaml: text,
        yamlError: parsed.message,
        issues: parsed.issues,
        savePhase: 'idle',
        saveProblem: null,
        saved: null,
      };
      return this.state;
    }
    this.state = {
      ...this.state,
      document: parsed.document,
      yaml: text,
      yamlError: null,
      issues: [],
      savePhase: 'idle',
      saveProblem: null,
      saved: null,
    };
    return this.state;
  }

  /** Ask the pool for the chosen host's agent choices again. */
  async reloadChoices(): Promise<TriggersViewState> {
    const token = (this.choicesToken += 1);
    const slug = this.state.document.run.host.trim();
    const directory = this.state.directory;

    if (slug === '') {
      this.state = { ...this.state, choices: { status: 'idle' } };
      return this.state;
    }

    const host = findHost(this.state.hosts, slug);
    if (!host) {
      this.state = {
        ...this.state,
        choices: {
          status: 'unknown-host',
          host: slug,
          directory,
          message: `No host with slug '${slug}' is enrolled. Choose an enrolled host.`,
        },
      };
      return this.state;
    }
    if (host.status === 'offline') {
      this.state = {
        ...this.state,
        choices: {
          status: 'offline',
          host: slug,
          directory,
          message: offlineHostMessage(slug, directory),
          problem: null,
        },
      };
      return this.state;
    }
    if (host.status === 'revoked') {
      this.state = {
        ...this.state,
        choices: { status: 'unavailable', host: slug, directory, message: revokedHostMessage(slug) },
      };
      return this.state;
    }
    if (host.status === 'pending_approval') {
      this.state = {
        ...this.state,
        choices: { status: 'unavailable', host: slug, directory, message: pendingHostMessage(slug) },
      };
      return this.state;
    }
    if (!host.executionEnabled) {
      this.state = {
        ...this.state,
        choices: {
          status: 'unavailable',
          host: slug,
          directory,
          message: `The host '${slug}' is connected only: host.execute is not granted, so it cannot run agents.`,
        },
      };
      return this.state;
    }

    this.state = {
      ...this.state,
      busy: true,
      choices: { status: 'loading', host: slug, directory },
    };
    try {
      const report = await this.transport.capabilities(host.id, directory);
      if (token !== this.choicesToken) return this.state;
      this.state = {
        ...this.state,
        busy: false,
        choices: {
          status: 'ready',
          host: slug,
          directory,
          report,
          options: agentOptionsFromReport(report),
        },
      };
    } catch (error) {
      if (token !== this.choicesToken) return this.state;
      const problem = presentProblem(error);
      const offline =
        problem.code === 'daemon_not_connected' ||
        problem.status === 503 ||
        problem.kind === 'network';
      this.state = {
        ...this.state,
        busy: false,
        choices: offline
          ? {
              status: 'offline',
              host: slug,
              directory,
              problem,
              message: offlineHostMessage(slug, directory),
            }
          : {
              status: 'unavailable',
              host: slug,
              directory,
              message: `The pool could not read what '${slug}' can run: ${problem.message}`,
            },
      };
    }
    return this.state;
  }

  /** Retry after an offline or failed capability call. */
  async retryChoices(): Promise<TriggersViewState> {
    return this.reloadChoices();
  }

  /**
   * Validate and install the document.
   *
   * Local shape and host checks run first, with the pool's field names. A pass
   * goes to `POST /v1/triggers/validate`; only a valid answer reaches
   * `POST /v1/triggers/install`. A refusal writes nothing and keeps the draft.
   */
  async save(): Promise<TriggersViewState> {
    const issues = preflightIssues({
      document: this.state.document,
      hosts: this.state.hosts,
      choices: this.state.choices,
    });
    if (issues.length > 0) {
      this.state = { ...this.state, issues, savePhase: 'refused', saveProblem: null };
      return this.state;
    }
    const project = this.state.selectedProjectId;
    if (project === null || project.trim() === '') {
      this.state = {
        ...this.state,
        savePhase: 'refused',
        saveProblem: {
          kind: 'app',
          title: 'Choose a project',
          message: 'A trigger belongs to one project. Choose the project it should be installed for.',
          requestId: null,
          status: null,
          code: 'project_required',
          type: null,
          retryAfterMs: null,
        },
      };
      return this.state;
    }

    this.state = {
      ...this.state,
      busy: true,
      savePhase: 'validating',
      issues: [],
      saveProblem: null,
      saved: null,
    };
    try {
      const validated = await this.transport.validate(this.state.document);
      if (!validated.valid) {
        this.state = {
          ...this.state,
          busy: false,
          savePhase: 'refused',
          issues: [...validated.errors],
        };
        return this.state;
      }
      this.state = { ...this.state, savePhase: 'installing' };
      const installed = await this.transport.install({
        project,
        ...cloneTriggerDocument(this.state.document),
      });
      this.state = {
        ...this.state,
        busy: false,
        savePhase: 'saved',
        saved: installed,
        issues: [],
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
}
