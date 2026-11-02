import { describe, expect, it } from 'vitest';
import {
  STATUS_DESCRIPTORS,
  STATUS_KINDS,
  describeStatus,
  isStatusKind,
  statusAriaLabel,
} from './status';

describe('status descriptors', () => {
  it('defines every kind once', () => {
    expect(STATUS_KINDS).toEqual(Object.keys(STATUS_DESCRIPTORS));
    expect(new Set(STATUS_KINDS).size).toBe(STATUS_KINDS.length);
  });

  it('never lets a status be colour alone: each carries an icon and a word', () => {
    for (const kind of STATUS_KINDS) {
      const status = STATUS_DESCRIPTORS[kind];
      expect(status.kind).toBe(kind);
      expect(status.word.trim().length).toBeGreaterThan(0);
      expect(status.description.trim().length).toBeGreaterThan(0);
      expect(status.icon).toMatch(/^M/);
      expect(status.icon.length).toBeGreaterThanOrEqual(8);
    }
  });

  it('gives each status a distinct word, so two states never read alike', () => {
    const words = STATUS_KINDS.map((kind) => STATUS_DESCRIPTORS[kind].word.toLowerCase());
    expect(new Set(words).size).toBe(words.length);
  });

  it('falls back to Unknown rather than throwing on an unread state', () => {
    expect(describeStatus('not-a-status').kind).toBe('unknown');
    expect(describeStatus(null).kind).toBe('unknown');
    expect(describeStatus(undefined).kind).toBe('unknown');
    expect(describeStatus('success').word).toBe('OK');
  });

  it('includes the word in the accessible label', () => {
    for (const kind of STATUS_KINDS) {
      const label = statusAriaLabel(kind);
      expect(label).toContain(STATUS_DESCRIPTORS[kind].word);
      expect(label).toContain(STATUS_DESCRIPTORS[kind].description);
    }
  });

  it('recognises only known kinds', () => {
    expect(isStatusKind('warning')).toBe(true);
    expect(isStatusKind('Warning')).toBe(false);
    expect(isStatusKind(42)).toBe(false);
  });
});
