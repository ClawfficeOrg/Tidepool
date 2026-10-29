<script lang="ts">
  /**
   * A problem-details failure, rendered for a person.
   *
   * The heading says what happened in plain words, the body is the pool's own
   * explanation, and the request id is always shown — including when there is
   * none, so nobody goes looking for a value that is not there. The status and
   * the machine code stay available underneath for a bug report.
   */
  import StatusBadge from './StatusBadge.svelte';
  import { requestIdLine, type PoolProblem } from '$lib/api/problem';

  interface Props {
    /** The failure to render. */
    problem: PoolProblem;
    /** Ask the pool again. */
    onRetry?: () => void;
  }

  let { problem, onRetry }: Props = $props();
  const requestLine = $derived(requestIdLine(problem));
</script>

<section
  class="tp-problem"
  role="alert"
  aria-labelledby="tp-problem-title"
  aria-describedby="tp-problem-request"
  data-problem-kind={problem.kind}
>
  <p class="tp-problem__status">
    <StatusBadge kind="danger" />
  </p>
  <h2 class="tp-problem__title" id="tp-problem-title">{problem.title}</h2>
  <p class="tp-problem__message">{problem.message}</p>
  <p class="tp-problem__request" id="tp-problem-request">{requestLine}</p>
  <dl class="tp-problem__facts">
    <div class="tp-problem__fact">
      <dt>Request id</dt>
      <dd><code>{problem.requestId ?? 'none'}</code></dd>
    </div>
    {#if problem.status !== null}
      <div class="tp-problem__fact">
        <dt>Status</dt>
        <dd><code>{problem.status}</code></dd>
      </div>
    {/if}
    {#if problem.code !== null}
      <div class="tp-problem__fact">
        <dt>Code</dt>
        <dd><code>{problem.code}</code></dd>
      </div>
    {/if}
  </dl>
  {#if onRetry}
    <button class="tp-button" type="button" onclick={() => onRetry?.()}>Try again</button>
  {/if}
</section>
