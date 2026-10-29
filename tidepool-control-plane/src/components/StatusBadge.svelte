<script lang="ts">
  /**
   * A status, rendered as colour + icon + word.
   *
   * The icon is decorative (`aria-hidden`): the word carries the meaning, so
   * the status is readable with colour off, in high contrast, and to a screen
   * reader. Clawffice design rule 8.
   */
  import { describeStatus, type StatusKind } from '$lib/status';

  interface Props {
    /** The status to render. */
    kind: StatusKind | string;
  }

  let { kind }: Props = $props();
  const status = $derived(describeStatus(kind));
</script>

<span class="tp-status" data-status={status.kind} title={status.description}>
  <svg
    class="tp-status__icon"
    viewBox="0 0 24 24"
    aria-hidden="true"
    focusable="false"
    role="presentation"
  >
    <path
      d={status.icon}
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
  <span class="tp-status__word">{status.word}</span>
</span>
