<script lang="ts">
  /**
   * One trigger, described.
   *
   * The card answers the three questions a person asks about a trigger, in this
   * order: what fires it, what it runs, and when it runs next. The answers come
   * from `triggerCards.ts`, which reads the same document the form edits, so the
   * card can never describe something other than what would be installed.
   *
   * "When" is the one answer the contract cannot give as a time, and the card
   * says so plainly rather than showing an empty column: a trigger has no
   * schedule, so its next run is the next matching event.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import type { TriggerCardModel } from './triggerCards';

  interface Props {
    /** The described trigger. */
    card: TriggerCardModel;
  }

  let { card }: Props = $props();
</script>

<article class="tp-trigger-card" data-trigger-card={card.key}>
  <header class="tp-trigger-card__header">
    <h3 class="tp-trigger-card__name">{card.name}</h3>
    {#if card.statusKind}
      <StatusBadge kind={card.statusKind} />
    {/if}
  </header>

  <p class="tp-trigger-card__status" data-trigger-card-status="true">
    {card.statusText}
  </p>

  <dl class="tp-trigger-card__answers">
    <div class="tp-trigger-card__answer" data-trigger-card-answer="fires">
      <dt>What fires it</dt>
      <dd>{card.fires}</dd>
    </div>
    <div class="tp-trigger-card__answer" data-trigger-card-answer="runs">
      <dt>What it runs</dt>
      <dd>{card.runs}</dd>
    </div>
    <div class="tp-trigger-card__answer" data-trigger-card-answer="next">
      <dt>When it runs next</dt>
      <dd>{card.nextRun}</dd>
    </div>
  </dl>

  <ul class="tp-trigger-card__facts" aria-label="Trigger settings">
    {#each card.facts as fact (fact.label)}
      <li class="tp-trigger-card__fact">
        <span class="tp-trigger-card__fact-label">{fact.label}</span>
        <code>{fact.value}</code>
      </li>
    {/each}
  </ul>
</article>

<style>
  .tp-trigger-card {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
    background: var(--tp-surface);
  }

  .tp-trigger-card__header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-trigger-card__name {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-heading);
    overflow-wrap: anywhere;
  }

  .tp-trigger-card__status {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-trigger-card__answers {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
  }

  .tp-trigger-card__answer {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-trigger-card__answer dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-trigger-card__answer dd {
    margin: 0;
    max-width: 78ch;
    overflow-wrap: anywhere;
  }

  .tp-trigger-card__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-trigger-card__fact {
    display: flex;
    gap: var(--tp-space-1);
    align-items: baseline;
    max-width: 100%;
  }

  .tp-trigger-card__fact-label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-trigger-card__fact code {
    overflow-wrap: anywhere;
  }
</style>
