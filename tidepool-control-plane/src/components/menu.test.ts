import { describe, expect, it } from 'vitest';
import {
  CLOSED_MENU,
  MENU_BREAKPOINT,
  escapeBelongsToField,
  escapeOutcome,
  menuReducer,
  owesFocus,
  toggleLabel,
  toggleWord,
} from './menu';

describe('navigation menu', () => {
  it('starts closed and owing focus to nobody', () => {
    expect(CLOSED_MENU).toEqual({ open: false, returnFocus: false });
  });

  it('opens and closes by the button, moving focus to neither', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'toggle' });
    expect(open).toEqual({ open: true, returnFocus: false });
    const closed = menuReducer(open, { type: 'toggle' });
    expect(closed).toEqual({ open: false, returnFocus: false });
  });

  it('closes on Escape and asks for focus back on the button', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'toggle' });
    const dismissed = menuReducer(open, { type: 'escape' });
    expect(dismissed).toEqual({ open: false, returnFocus: true });
    expect(owesFocus(dismissed)).toBe(true);
  });

  it('ignores Escape when the menu is already closed', () => {
    expect(menuReducer(CLOSED_MENU, { type: 'escape' })).toBe(CLOSED_MENU);
  });

  it('closes without claiming focus when a link is followed', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'open' });
    const afterLink = menuReducer(open, { type: 'navigate' });
    expect(afterLink).toEqual({ open: false, returnFocus: false });
    expect(owesFocus(afterLink)).toBe(false);
  });

  it('clears a pending focus request on the next open', () => {
    const dismissed = menuReducer(
      menuReducer(CLOSED_MENU, { type: 'toggle' }),
      { type: 'escape' },
    );
    expect(menuReducer(dismissed, { type: 'toggle' })).toEqual({
      open: true,
      returnFocus: false,
    });
  });

  it('treats open, close and navigate as no-ops when they change nothing', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'open' });
    expect(menuReducer(open, { type: 'open' })).toBe(open);
    expect(menuReducer(CLOSED_MENU, { type: 'close' })).toBe(CLOSED_MENU);
    expect(menuReducer(CLOSED_MENU, { type: 'navigate' })).toBe(CLOSED_MENU);
  });

  it('names the button after what pressing it does', () => {
    expect(toggleWord(false)).toBe('Menu');
    expect(toggleWord(true)).toBe('Close');
    expect(toggleLabel(false)).toBe('Open navigation');
    expect(toggleLabel(true)).toBe('Close navigation');
  });

  it('collapses at the tablet breakpoint the shell uses', () => {
    expect(MENU_BREAKPOINT).toBe('(max-width: 52rem)');
  });

  it('leaves Escape to a field that is using it', () => {
    const field = (tagName: string, extra: Record<string, unknown> = {}): EventTarget =>
      ({ tagName, ...extra }) as unknown as EventTarget;
    expect(escapeBelongsToField(field('INPUT'))).toBe(true);
    expect(escapeBelongsToField(field('textarea'))).toBe(true);
    expect(escapeBelongsToField(field('SELECT'))).toBe(true);
    expect(escapeBelongsToField(field('DIV', { isContentEditable: true }))).toBe(true);
    expect(escapeBelongsToField(field('DIV'))).toBe(false);
    expect(escapeBelongsToField(field('BUTTON'))).toBe(false);
    expect(escapeBelongsToField(field('A'))).toBe(false);
    expect(escapeBelongsToField(null)).toBe(false);
  });

  it('closes on Escape and hands focus back to the button', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'toggle' });
    const body = { tagName: 'DIV' } as unknown as EventTarget;
    expect(escapeOutcome(open, body)).toEqual({
      state: { open: false, returnFocus: true },
      focusButton: true,
      handled: true,
    });
  });

  it('leaves Escape alone when the menu is closed', () => {
    const body = { tagName: 'DIV' } as unknown as EventTarget;
    expect(escapeOutcome(CLOSED_MENU, body)).toEqual({
      state: CLOSED_MENU,
      focusButton: false,
      handled: false,
    });
  });

  it('leaves Escape to a field while the menu is open', () => {
    const open = menuReducer(CLOSED_MENU, { type: 'toggle' });
    const field = { tagName: 'INPUT' } as unknown as EventTarget;
    expect(escapeOutcome(open, field)).toEqual({
      state: open,
      focusButton: false,
      handled: false,
    });
  });
});
