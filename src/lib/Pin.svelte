<script lang="ts">
  import Puzzle from '@lucide/svelte/icons/puzzle'
  import type { ModuleUis } from '../modules'
  import Cover from './Cover.svelte'
  import Dot from './Dot.svelte'
  import ModuleIcon from './ModuleIcon.svelte'
  import More from './More.svelte'
  import Marquee from './Marquee.svelte'
  import Sections from './Sections.svelte'
  import { icons } from './icons'
  import {
    compactMarks,
    onlyItem,
    MAX_COMPACT_COMPANIONS,
    MAX_VERTICAL_COMPANIONS,
    mostUrgent,
    ringing,
    type Rect,
    type Section,
  } from './content'

  // Space on each side of the closed pin's summary, part of `measured`.
  const PIN_PADDING = 12

  // A pinned module's mini-notch, next to the notch (DF-0012). Closed, it
  // shows the essentials only; hovered or alerting, the whole module.
  let {
    section,
    rect,
    corners,
    open,
    spotlit,
    ontextreadtime,
    vertical,
    moduleUis,
    measured = $bindable(),
    onenter,
    onleave,
    onclick,
    onaction,
    onmenu,
  }: {
    section: Section
    // Where and how big, from App.svelte (it also sets the window region).
    rect: Rect
    corners: string
    open: boolean
    // Just changed (the volume keys): its value for a moment, even when quiet.
    spotlit: boolean
    // How long its compact text needs to be read once (Marquee).
    ontextreadtime: (module: string, ms: number) => void
    // On the left / right edges of the screen: thin, dots or icon only.
    vertical: boolean
    moduleUis: ModuleUis
    // Width the closed pin needs for its summary, padding included; unset
    // until measured.
    measured?: number
    onenter: () => void
    onleave: () => void
    // A click on the pin: its module is acknowledged.
    // A click on the pin, on one of its items (open) if the click was on one.
    onclick: (item?: string) => void
    onaction: (itemId: string, actionId: string) => void
    // Right click on the pin (its module's id).
    onmenu: (module: string, item?: string) => void
  } = $props()

  const top = $derived(mostUrgent(section.items))
  const compactLabel = $derived(top ? (top.compact?.label ?? top.label) : '')
  // A spotlight shows the text of a quiet item, if it has one: an item with
  // nothing to say (a media module with nothing playing) keeps its icon.
  const quiet = $derived(!!top?.quiet && (!spotlit || !compactLabel))
  const dotted = $derived(
    compactMarks(section.items, vertical ? MAX_VERTICAL_COMPANIONS : MAX_COMPACT_COMPANIONS),
  )
  // The item's own icon (the volume's level), else the module's.
  const Icon = $derived(
    top?.icon ? icons[top.icon] : (moduleUis[section.module]?.icon ?? Puzzle),
  )

  // Thin on the left / right edges, unless it shows its value for a moment,
  // or its item asks for it (a running stopwatch): then a small horizontal
  // bubble (DF-0027).
  const bubble = $derived(vertical && (spotlit || !!top?.bubble) && !open)
  const thin = $derived(vertical && !open && !bubble)

  let summaryWidth = $state(0)
  $effect(() => {
    if (!open && !thin && summaryWidth > 0) {
      measured = Math.ceil(summaryWidth) + 2 * PIN_PADDING
    }
  })
</script>

<!-- Not a <button>: it holds the items' buttons once open. -->
<div
  role="button"
  tabindex="-1"
  class="pin"
  class:open
  class:vertical={thin}
  class:after-notch={section.pin === 'right'}
  class:before-notch={section.pin === 'left'}
  class:ringing={ringing([section])}
  style:left="{rect.x}px"
  style:top="{rect.y}px"
  style:width="{rect.width}px"
  style:height="{rect.height}px"
  style:border-radius={corners}
  style:padding={thin ? '0' : `0 ${open ? 16 : PIN_PADDING}px`}
  onmouseenter={onenter}
  onmouseleave={onleave}
  onclick={(e) =>
    onclick((e.target as Element).closest<HTMLElement>('[data-item]')?.dataset.item)}
  onkeydown={(e) => e.key === 'Enter' && onclick()}
  oncontextmenu={(e) => {
    e.preventDefault()
    // The item under the cursor (open), else its only one (a companion).
    const row = (e.target as Element).closest<HTMLElement>('[data-item]')?.dataset.item
    onmenu(section.module, row ?? onlyItem(section.items))
  }}
>
  {#if open}
    <Sections list={[section]} height={rect.height} {moduleUis} {onaction} />
  {:else if thin}
    {#if dotted.shown.length > 0}
      <span class="dots column">
        {#each dotted.shown as item (item.id)}
          <Dot {item} compact />
        {/each}
        {#if dotted.hidden > 0}<More count={dotted.hidden} />{/if}
      </span>
    {:else if top && top.stack.length > 0}
      <span class="stacked">
        {#each top.stack as line, i (i)}<span>{line}</span>{/each}
      </span>
    {:else if top && top.image !== null}
      <Cover src={top.image} icon={moduleUis[section.module]?.icon ?? Puzzle} dimmed={top.dimmed} />
    {:else}
      <ModuleIcon icon={Icon} item={top} {onaction} />
    {/if}
  {:else if top}
    <!-- At the closed width from the start, against the pin's side by the
         notch: while the pin folds back around it, it does not move. -->
    <span class="closed" style:width="{Math.max(0, rect.width - 2 * PIN_PADDING)}px">
      <span class="summary" bind:offsetWidth={summaryWidth}>
        {#if dotted.shown.length > 0}
          <span class="dots">
            {#each dotted.shown as item (item.id)}
              <Dot {item} compact />
            {/each}
            {#if dotted.hidden > 0}<More count={dotted.hidden} />{/if}
          </span>
        {/if}
        {#if quiet && top.image !== null}
          <Cover src={top.image} icon={moduleUis[section.module]?.icon ?? Puzzle} dimmed={top.dimmed} />
        {:else if quiet}
          <ModuleIcon icon={Icon} item={top} {onaction} />
        {:else if top.slider}
          <!-- A slider (the volume): its icon, then its value as a number;
               its compact text instead, faint, if it has one (muted). -->
          <ModuleIcon icon={Icon} item={top} {onaction} />
          {#if top.compact}
            <span class="label muted">{top.compact.label}</span>
          {:else}
            <span class="value">{Math.round(top.slider.value)}</span>
          {/if}
        {:else}
          {#if top.image !== null}
            <Cover src={top.image} icon={moduleUis[section.module]?.icon ?? Puzzle} dimmed={top.dimmed} />
          {/if}
          {#if compactLabel}
            <Marquee
              text={compactLabel}
              scroll={top.scroll}
              dimmed={top.dimmed}
              onturn={(ms) => ontextreadtime(section.module, ms)}
            />
          {/if}
          {#if top.compact?.title}<span class="muted">{top.compact.title}</span>{/if}
        {/if}
      </span>
    </span>
  {/if}
</div>

<style>
  .pin {
    position: absolute;
    box-sizing: border-box;
    overflow: hidden;
    pointer-events: auto;
    background: var(--notch-fill);
    outline: 1px solid var(--notch-border);
    outline-offset: -1px;
    transition:
      left var(--duration) ease,
      top var(--duration) ease,
      width var(--duration) ease,
      height var(--duration) ease,
      border-radius var(--duration) ease;
  }

  /* Closed, its summary or icon is centered. Beside the notch (top and
     bottom edges), its box keeps to the side by the notch, the one that
     stays still while the pin opens and closes; a thin pin (left and right
     edges) keeps its icon centered. */
  .pin:not(.open) {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pin.after-notch:not(.open, .vertical) {
    justify-content: flex-start;
  }

  .pin.before-notch:not(.open, .vertical) {
    justify-content: flex-end;
  }

  .closed {
    display: flex;
    flex-shrink: 0;
    justify-content: center;
    height: 100%;
  }

  /* The essentials, at their natural width: measured to size the pin. */
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    width: max-content;
    height: 100%;
    white-space: nowrap;
  }

  /* Digits of equal width: the pin does not jitter while the value moves. */
  .value {
    font-variant-numeric: tabular-nums;
  }

  /* A long text (a song title) stops the pin from growing too wide. */
  .summary :global(.label) {
    max-width: 180px;
  }
</style>
