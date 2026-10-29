/**
 * Status: the icon and the word that go beside every colour.
 *
 * Clawffice design rule 8 is that state is never colour alone. A status in this
 * app is therefore a `word` (read by everyone and by a screen reader) plus an
 * `icon` (decorative, hidden from assistive technology because the word already
 * says it) plus a colour role drawn from the palette. `StatusBadge.svelte`
 * renders all three, and `status.test.ts` fails if a status loses its word or
 * its icon.
 */

/** The statuses the control plane renders. */
export type StatusKind =
  | 'success'
  | 'info'
  | 'warning'
  | 'danger'
  | 'pending'
  | 'neutral'
  | 'unknown';

/** One status, in every form it is rendered. */
export interface StatusDescriptor {
  /** Stable kind, also used as the `data-status` attribute. */
  kind: StatusKind;
  /** The word shown beside the icon. Never empty. */
  word: string;
  /** Visual description, used as a tooltip, never as the accessible name. */
  description: string;
  /** 24x24 SVG path drawn with the palette's status colour. */
  icon: string;
}

/**
 * The status table.
 *
 * Paths are simple 24x24 glyphs; the design system's icon set (Iconoir) is for
 * icons that decorate prose, not for state, which must survive without colour
 * and without a font.
 */
export const STATUS_DESCRIPTORS: Readonly<Record<StatusKind, StatusDescriptor>> = {
  success: {
    kind: 'success',
    word: 'OK',
    description: 'Working as intended',
    icon: 'M4 12.5l5 5L20 6.5',
  },
  info: {
    kind: 'info',
    word: 'Info',
    description: 'Information, no action needed',
    icon: 'M12 10.5v7M12 6.5h.01',
  },
  warning: {
    kind: 'warning',
    word: 'Warning',
    description: 'Needs attention',
    icon: 'M12 4l9 16H3l9-16zM12 10v4M12 17h.01',
  },
  danger: {
    kind: 'danger',
    word: 'Failed',
    description: 'Did not succeed',
    icon: 'M6 6l12 12M18 6L6 18',
  },
  pending: {
    kind: 'pending',
    word: 'Pending',
    description: 'Waiting to start or finish',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM12 7.5V12l3 2',
  },
  neutral: {
    kind: 'neutral',
    word: 'Inactive',
    description: 'Turned off or not in use',
    icon: 'M5 12h14',
  },
  unknown: {
    kind: 'unknown',
    word: 'Unknown',
    description: 'The state could not be read',
    icon: 'M9.5 9a2.5 2.5 0 115 .5c0 1.5-2.5 1.5-2.5 3.5M12 17h.01',
  },
};

/** Every status kind, in presentation order. */
export const STATUS_KINDS: readonly StatusKind[] = [
  'success',
  'info',
  'warning',
  'danger',
  'pending',
  'neutral',
  'unknown',
];

/** True when `value` names a known status. */
export function isStatusKind(value: unknown): value is StatusKind {
  return typeof value === 'string' && Object.hasOwn(STATUS_DESCRIPTORS, value);
}

/** Look a status up, falling back to `unknown` rather than throwing. */
export function describeStatus(kind: string | null | undefined): StatusDescriptor {
  return isStatusKind(kind) ? STATUS_DESCRIPTORS[kind] : STATUS_DESCRIPTORS.unknown;
}

/**
 * The accessible label for a status.
 *
 * Read out where the word is not adjacent, e.g. on a compact row whose status
 * is at the end. Always contains the word.
 */
export function statusAriaLabel(kind: string | null | undefined): string {
  const status = describeStatus(kind);
  return `${status.word}: ${status.description}`;
}
