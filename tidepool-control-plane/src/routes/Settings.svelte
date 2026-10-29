<script lang="ts">
  /**
   * The settings route.
   *
   * The view lives in `./settings/`; this file only keeps the router's import
   * stable and passes the shell's pool, session, role and preferences through.
   */
  import SettingsPage from './settings/SettingsPage.svelte';
  import type { PoolConfig } from '$lib/api/pool';
  import type { MembersKeysState, MembersKeysTransport } from '$lib/keys';
  import type { Role } from '../auth/types';
  import type { PaletteName } from '$lib/design/tokens';
  import type { ControlPlanePreferences } from './settings/preferences';

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
    /** The palette the shell is rendering. */
    palette?: PaletteName;
    /** The preferences in effect. */
    preferences?: ControlPlanePreferences;
    /** True when the pool itself served the page. */
    servedByPool?: boolean;
    /** Called with a new pool URL. */
    onSavePoolUrl?: (url: string) => void;
    /** Called with a palette choice. */
    onPaletteSelect?: (name: PaletteName) => void;
    /** Called with any other preference change. */
    onPreferenceChange?: (patch: Partial<ControlPlanePreferences>) => void;
    /** A rejected pool URL, in words. */
    poolUrlProblem?: string | null;
    /** A saved pool URL notice. */
    poolUrlNotice?: string | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial,
    transport = null,
    palette,
    preferences,
    servedByPool = false,
    onSavePoolUrl,
    onPaletteSelect,
    onPreferenceChange,
    poolUrlProblem = null,
    poolUrlNotice = null,
  }: Props = $props();
</script>

<SettingsPage
  {pool}
  {token}
  {role}
  {initial}
  {transport}
  {palette}
  {preferences}
  {servedByPool}
  {onSavePoolUrl}
  {onPaletteSelect}
  {onPreferenceChange}
  {poolUrlProblem}
  {poolUrlNotice}
/>
