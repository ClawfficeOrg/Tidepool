<script lang="ts">
  /**
   * A host status, rendered as colour + icon + word.
   *
   * The generic `StatusBadge` says "Pending"; a host needs the words an
   * operator acts on ("Approval required"), so this badge reads its table from
   * `$lib/hosts` and keeps the same accessible pattern: the icon is decorative
   * and the word carries the meaning.
   */
  import { describeHostStatus } from '$lib/hosts';

  interface Props {
    /** The status the pool reported. */
    status: string | null | undefined;
  }

  let { status }: Props = $props();
  const descriptor = $derived(describeHostStatus(status));
</script>

<span
  class="tp-status"
  data-status={descriptor.tone}
  data-host-status={descriptor.status ?? 'unknown'}
  title={descriptor.description}
>
  <svg
    class="tp-status__icon"
    viewBox="0 0 24 24"
    aria-hidden="true"
    focusable="false"
    role="presentation"
  >
    <path
      d={descriptor.icon}
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
  <span class="tp-status__word">{descriptor.word}</span>
</span>
