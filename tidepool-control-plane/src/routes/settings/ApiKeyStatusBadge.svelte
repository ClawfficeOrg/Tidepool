<script lang="ts">
  /**
   * An API key status, rendered as colour + icon + word.
   *
   * Same accessible pattern as every other status in the app: the icon is
   * decorative and the word carries the meaning. Revoked is a state, not an
   * absence: the badge stays on the row so a person sees what happened.
   */
  import { describeApiKeyStatus } from '$lib/keys';

  interface Props {
    /** The status the pool reported. */
    status: string | null | undefined;
  }

  let { status }: Props = $props();
  const descriptor = $derived(describeApiKeyStatus(status));
</script>

<span
  class="tp-status"
  data-status={descriptor.tone}
  data-key-status={descriptor.status ?? 'unknown'}
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
