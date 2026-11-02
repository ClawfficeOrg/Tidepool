<script lang="ts">
  /**
   * The result of verifying one connection.
   *
   * Every check is listed, passed or failed, with its own sentence. The summary
   * names the first failing check, so "it did not work" is never the answer —
   * the operator is told which check failed, in the provider's own words.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import type { VerificationReport } from '$lib/connections';

  interface Props {
    /** The report to render. */
    report: VerificationReport;
  }

  let { report }: Props = $props();
</script>

<section
  class="tp-verification"
  data-verification={report.ok ? 'passed' : 'failed'}
  role={report.ok ? 'status' : 'alert'}
  aria-labelledby={`tp-verification-${report.connectionId}`}
>
  <h3 class="tp-verification__title" id={`tp-verification-${report.connectionId}`}>
    {report.ok ? 'Verification passed' : 'Verification failed'}
  </h3>
  <p class="tp-verification__summary" data-verification-summary="true">{report.summary}</p>

  <ul class="tp-verification__checks">
    {#each report.checks as check (check.id)}
      <li class="tp-verification__check" data-check={check.id} data-check-ok={check.ok ? 'true' : 'false'}>
        <StatusBadge kind={check.ok ? 'success' : 'danger'} />
        <div class="tp-verification__body">
          <p class="tp-verification__label">{check.label}</p>
          <p class="tp-verification__detail">{check.detail}</p>
        </div>
      </li>
    {/each}
  </ul>
</section>

<style>
  .tp-verification {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-verification[data-verification='failed'] {
    border-color: var(--tp-status-danger);
  }

  .tp-verification__title {
    margin: 0;
    color: var(--tp-text-strong);
  }

  .tp-verification__summary {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-verification__checks {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-verification__check {
    display: flex;
    gap: var(--tp-space-1);
    align-items: flex-start;
  }

  .tp-verification__body {
    display: grid;
    gap: 2px;
  }

  .tp-verification__label {
    margin: 0;
    color: var(--tp-text-strong);
  }

  .tp-verification__detail {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }
</style>
