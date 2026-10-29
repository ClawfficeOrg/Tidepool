<script lang="ts">
  /**
   * The shell's navigation.
   *
   * Links, not buttons: every page has an address, so it can be bookmarked and
   * opened in a new tab. The current page is marked with `aria-current`.
   */
  import { routeHref, type RouteDefinition } from '$lib/router';

  interface Props {
    /** The route table. */
    routes: readonly RouteDefinition[];
    /** The path currently shown. */
    current: string;
  }

  let { routes, current }: Props = $props();
</script>

<nav class="tp-nav" aria-label="Control plane">
  <ul class="tp-nav__list">
    {#each routes as route (route.id)}
      <li class="tp-nav__item">
        <a
          class="tp-nav__link"
          href={routeHref(route.path)}
          aria-current={route.path === current ? 'page' : undefined}
          data-route={route.id}
        >
          <span class="tp-nav__label">{route.label}</span>
          <span class="tp-nav__description">{route.description}</span>
        </a>
      </li>
    {/each}
  </ul>
</nav>
