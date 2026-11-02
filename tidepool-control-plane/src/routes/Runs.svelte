<script lang="ts">
  /**
   * The runs route.
   *
   * The view lives in `./runs/`; this file only keeps the router's import
   * stable and passes the shell's pool, session and role through.
   */
  import RunsPage from './runs/RunsPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { RunsTransport, RunsViewState } from '$lib/runs';
  import type { Role } from '../auth/types';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role. */
    role?: Role | null;
    /** An already-loaded state, for tests and resumed flows. */
    initial?: RunsViewState;
    /** Transport override, for tests. */
    transport?: RunsTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
  }: Props = $props();
</script>

<RunsPage {pool} {token} {role} {initial} {transport} />
