<script lang="ts">
  /**
   * The hosts route.
   *
   * The view lives in `./hosts/`; this file only keeps the router's import
   * stable and passes the shell's pool, session and role through.
   */
  import HostsPage from './hosts/HostsPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { HostsTransport, HostsViewState } from '$lib/hosts';
  import type { Role } from '../auth/types';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role. */
    role?: Role | null;
    /** An already-loaded state, for tests and resumed flows. */
    initial?: HostsViewState;
    /** Transport override, for tests. */
    transport?: HostsTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
  }: Props = $props();
</script>

<HostsPage {pool} {token} {role} {initial} {transport} />
