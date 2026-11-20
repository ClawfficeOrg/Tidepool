<script lang="ts">
  /**
   * The shell's navigation, and the menu that carries it on a narrow screen.
   *
   * Links, not buttons: every page has an address, so it can be bookmarked and
   * opened in a new tab. The current page is marked with `aria-current`.
   *
   * Below tablet width the sidebar column is gone, so a button above the list
   * opens and closes it. It is one component and one list either way, so the
   * menu cannot fall behind the sidebar. The button carries
   * `aria-expanded`/`aria-controls`; Escape closes the menu and puts focus back
   * on the button, and following a link closes it and leaves focus on the page.
   */
  import { tick, untrack } from 'svelte';
  import { routeHref, type RouteDefinition } from '$lib/router';
  import {
    CLOSED_MENU,
    escapeOutcome,
    menuReducer,
    toggleLabel,
    toggleWord,
    type MenuAction,
    type MenuState,
  } from './menu';

  interface Props {
    /** The route table. */
    routes: readonly RouteDefinition[];
    /** The path currently shown. */
    current: string;
  }

  let { routes, current }: Props = $props();

  /** The id the button controls; one menu per shell. */
  const PANEL_ID = 'tp-nav';

  let menu = $state<MenuState>(CLOSED_MENU);
  let toggle = $state<HTMLButtonElement | null>(null);

  /** Move the menu one step. */
  function dispatch(action: MenuAction): void {
    menu = menuReducer(menu, action);
  }

  /** Put focus back on the button once the panel has been re-rendered. */
  async function returnFocus(): Promise<void> {
    await tick();
    toggle?.focus();
  }

  /** The button: open when closed, close when open. Focus stays on it. */
  function press(): void {
    dispatch({ type: 'toggle' });
  }

  /** Escape closes an open menu, unless a field is using the key. */
  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return;
    const outcome = escapeOutcome(menu, event.target);
    if (!outcome.handled) return;
    event.preventDefault();
    menu = outcome.state;
    if (outcome.focusButton) void returnFocus();
  }

  /** Following a link means the page moved, so the menu has done its job. */
  function follow(): void {
    dispatch({ type: 'navigate' });
  }

  /**
   * A route change from anywhere else (the address bar, a link outside the
   * menu) means the menu no longer reflects where the person is going. Only
   * `current` is tracked, so closing the menu cannot re-trigger this.
   */
  $effect(() => {
    void current;
    untrack(() => {
      if (menu.open) dispatch({ type: 'navigate' });
    });
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="tp-sidebar">
  <button
    class="tp-menu-toggle"
    type="button"
    bind:this={toggle}
    aria-expanded={menu.open}
    aria-controls={PANEL_ID}
    aria-label={toggleLabel(menu.open)}
    data-menu-toggle
    onclick={press}
  >
    <span class="tp-menu-toggle__bars" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
    </span>
    <span class="tp-menu-toggle__word">{toggleWord(menu.open)}</span>
  </button>

  <nav class="tp-nav" id={PANEL_ID} aria-label="Control plane" data-open={menu.open}>
    <ul class="tp-nav__list">
      {#each routes as route (route.id)}
        <li class="tp-nav__item">
          <a
            class="tp-nav__link"
            href={routeHref(route.path)}
            aria-current={route.path === current ? 'page' : undefined}
            data-route={route.id}
            onclick={follow}
          >
            <span class="tp-nav__label">{route.label}</span>
            <span class="tp-nav__description">{route.description}</span>
          </a>
        </li>
      {/each}
    </ul>
  </nav>
</div>
