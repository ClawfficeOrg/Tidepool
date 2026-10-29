<script lang="ts">
  /**
   * The connections route.
   *
   * The view lives in `./connections/`; this file only keeps the router's
   * import stable and passes the shell's pool, session and role through.
   */
  import ConnectionsPage from './connections/ConnectionsPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { ConnectionsTransport, ConnectionsViewState } from '$lib/connections';
  import type { Role } from '../auth/types';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role. */
    role?: Role | null;
    /** An already-loaded state, for tests and resumed flows. */
    initial?: ConnectionsViewState;
    /** Transport override, for tests. */
    transport?: ConnectionsTransport | null;
    /** The origin the page last showed, for the callback change banner. */
    previousPoolUrl?: string | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
    previousPoolUrl = null,
  }: Props = $props();
</script>

<ConnectionsPage {pool} {token} {role} {initial} {transport} {previousPoolUrl} />
