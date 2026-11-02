<script lang="ts">
  /** The overview page. A placeholder until the pool views land. */
  import StatusBadge from '../components/StatusBadge.svelte';
  import RolePanel from '../auth/RolePanel.svelte';
  import type { Role } from '../auth/types';

  interface Props {
    /** The signed-in operator's role, when there is one. */
    role?: Role | null;
    /** The operator's display name. */
    name?: string;
    /** The organization name, when known. */
    organization?: string | null;
  }

  let { role = null, name = '', organization = null }: Props = $props();
</script>

<section class="tp-page" aria-labelledby="tp-overview-title">
  <h1 class="tp-page__title" id="tp-overview-title">Overview</h1>
  <p class="tp-page__lede">
    This control plane is scaffolded on the Clawffice design tokens. The pool
    views are being built in the order the plan sets: sign-in, hosts,
    connections, projects, triggers, runs, members, rooms, settings.
  </p>
  <p class="tp-page__state">
    <StatusBadge kind="pending" />
    <span>Hosts, runs and rooms appear once their views land.</span>
  </p>
</section>

{#if role}
  <RolePanel {role} {name} {organization} />
{/if}
