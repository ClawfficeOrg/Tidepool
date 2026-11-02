/**
 * The committed design snapshot must match a sibling `clawffice-design`
 * checkout, file for file.
 *
 * The control plane renders from the snapshot so an offline clone builds; this
 * test is what makes "consumes clawffice-design" a fact rather than a comment.
 * It skips when no checkout is present (CI without the design repository,
 * which is how the design system is consumed: it is not vendored into npm).
 */

import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import snapshot from './clawffice-design.snapshot.json';

/** Find a sibling design checkout, matching `snapshot.mjs`. */
function locateDesignDir(): string | null {
  const appRoot = resolve(process.cwd());
  const candidates = [
    process.env['CLAWFFICE_DESIGN_DIR'],
    join(appRoot, '..', 'clawffice-design'),
    join(appRoot, '..', '..', 'clawffice-design'),
  ].filter((candidate): candidate is string => typeof candidate === 'string');
  for (const candidate of candidates) {
    if (existsSync(join(candidate, 'tokens', 'core.json'))) return resolve(candidate);
  }
  return null;
}

const designDir = locateDesignDir();

/** Read a token file out of the checkout. */
function token(relative: string): unknown {
  if (!designDir) throw new Error('no design checkout');
  return JSON.parse(readFileSync(join(designDir, relative), 'utf8'));
}

describe.skipIf(designDir === null)('clawffice-design drift', () => {
  it('keeps the five palette files verbatim', () => {
    for (const name of ['biolume', 'boiled', 'deep-tide', 'phosphor', 'sea-glass']) {
      expect(snapshot.palettes[name as keyof typeof snapshot.palettes]).toEqual(
        token(`tokens/palettes/${name}.json`),
      );
    }
  });

  it('keeps the shared scale verbatim', () => {
    expect(snapshot.core).toEqual(token('tokens/core.json'));
  });

  it('keeps the Tidepool app tokens verbatim', () => {
    expect(snapshot.app).toEqual(token('tokens/apps/tidepool.json'));
  });

  it('keeps the pool cell sprite verbatim', () => {
    const cells = token('tokens/cells.json') as { sprites: { pool: unknown } };
    expect(snapshot.cells.pool).toEqual(cells.sprites.pool);
  });

  it('names every file it was generated from', () => {
    expect(snapshot.source.files).toHaveLength(3 + 5);
    expect(snapshot.source.commit).toMatch(/^[0-9a-f]{7,40}$|^unknown$/);
  });
});
