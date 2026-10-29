<script lang="ts">
  /**
   * The triggers route.
   *
   * The view lives in `./triggers/`; this file only keeps the router's import
   * stable and passes the shell's pool, session and role through.
   */
  import TriggersPage from './triggers/TriggersPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { TriggersTransport, TriggersViewState } from '$lib/triggers';
  import type { Role } from '../auth/types';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role. */
    role?: Role | null;
    /** An already-loaded state, for tests and resumed flows. */
    initial?: TriggersViewState;
    /** Transport override, for tests. */
    transport?: TriggersTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
  }: Props = $props();
</script>

<TriggersPage {pool} {token} {role} {initial} {transport} />
