<script lang="ts">
  /**
   * The settings route.
   *
   * The view lives in `./settings/`; this file only keeps the router's import
   * stable and passes the shell's pool, session and role through.
   */
  import SettingsPage from './settings/SettingsPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { MembersKeysState, MembersKeysTransport } from '$lib/keys';
  import type { Role } from '../auth/types';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role. */
    role?: Role | null;
    /** An already-loaded state, for tests and resumed flows. */
    initial?: MembersKeysState;
    /** Transport override, for tests. */
    transport?: MembersKeysTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
  }: Props = $props();
</script>

<SettingsPage {pool} {token} {role} {initial} {transport} />
