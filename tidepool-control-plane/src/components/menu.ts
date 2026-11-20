/**
 * The narrow-width navigation menu, as a pure state machine.
 *
 * Below tablet width the sidebar collapses into a button that opens the same
 * navigation. The button and the panel are one element in the markup, so the
 * menu cannot drift from the sidebar: the panel is the sidebar, shown or
 * hidden. Keeping the open/closed and focus-return rules here (rather than in
 * the component) means they can be asserted without a browser.
 */

/**
 * The width below which the sidebar collapses into the menu.
 *
 * 52rem is the same breakpoint `src/app.css` uses for the single-column shell,
 * so the layout and the menu change together.
 */
export const MENU_BREAKPOINT = '(max-width: 52rem)';

/** Whether the menu is open, and whether focus owes the button a visit. */
export interface MenuState {
  /** True when the panel is showing. */
  readonly open: boolean;
  /** True when closing should put focus back on the button. */
  readonly returnFocus: boolean;
}

/** The menu before anyone has touched it. */
export const CLOSED_MENU: MenuState = Object.freeze({
  open: false,
  returnFocus: false,
});

/** What happened to the menu. */
export type MenuAction =
  /** The button was pressed. */
  | { readonly type: 'toggle' }
  /** The panel was opened directly. */
  | { readonly type: 'open' }
  /** The panel was dismissed without moving anywhere. */
  | { readonly type: 'close' }
  /** A navigation link was followed. */
  | { readonly type: 'navigate' }
  /** Escape was pressed. */
  | { readonly type: 'escape' };

/**
 * Apply one action.
 *
 * Escape is the only action that asks for focus back on the button: a person
 * who dismisses the menu with the keyboard stays where they were. Following a
 * link moves focus to the page, so the button must not reclaim it.
 */
export function menuReducer(state: MenuState, action: MenuAction): MenuState {
  switch (action.type) {
    case 'toggle':
      return { open: !state.open, returnFocus: false };
    case 'open':
      return state.open ? state : { open: true, returnFocus: false };
    case 'close':
      return state.open ? CLOSED_MENU : state;
    case 'navigate':
      return state.open ? CLOSED_MENU : state;
    case 'escape':
      return state.open ? { open: false, returnFocus: true } : state;
  }
}

/** The word on the button: what pressing it does. */
export function toggleWord(open: boolean): string {
  return open ? 'Close' : 'Menu';
}

/** The button's accessible name, which says what it controls. */
export function toggleLabel(open: boolean): string {
  return open ? 'Close navigation' : 'Open navigation';
}

/** True when `state` asks the caller to move focus to the button. */
export function owesFocus(state: MenuState): boolean {
  return state.returnFocus;
}

/**
 * True when Escape was pressed inside a field that uses it itself.
 *
 * A menu that swallowed Escape from a text field would both interrupt typing
 * and move focus away from where the person was working, so the menu ignores
 * the key there.
 */
export function escapeBelongsToField(target: EventTarget | null): boolean {
  if (target === null || typeof target !== 'object') return false;
  const element = target as { tagName?: unknown; isContentEditable?: unknown };
  const tag = typeof element.tagName === 'string' ? element.tagName.toUpperCase() : '';
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return element.isContentEditable === true;
}

/** What pressing Escape did. */
export interface EscapeOutcome {
  /** The menu after the key. Unchanged when the key was not the menu's. */
  readonly state: MenuState;
  /** True when the caller must put focus on the menu button. */
  readonly focusButton: boolean;
  /** True when the key was the menu's to handle. */
  readonly handled: boolean;
}

/**
 * Decide what Escape does.
 *
 * It closes an open menu and hands focus back to the button. A closed menu, or
 * a field that is using the key itself, leaves both alone: the outcome then
 * says the key was not the menu's to handle.
 */
export function escapeOutcome(state: MenuState, target: EventTarget | null): EscapeOutcome {
  if (!state.open || escapeBelongsToField(target)) {
    return { state, focusButton: false, handled: false };
  }
  const next = menuReducer(state, { type: 'escape' });
  return { state: next, focusButton: owesFocus(next), handled: true };
}
