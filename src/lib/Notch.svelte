<script lang="ts">
  import Puzzle from '@lucide/svelte/icons/puzzle'
  import { cubicOut } from 'svelte/easing'
  import { fade } from 'svelte/transition'
  import type { ModuleUis } from '../modules'
  import Cover from './Cover.svelte'
  import Dot from './Dot.svelte'
  import Marquee from './Marquee.svelte'
  import ModuleIcon from './ModuleIcon.svelte'
  import More from './More.svelte'
  import Sections from './Sections.svelte'
  import { icons } from './icons'
  import {
    compactMarks,
    compactSection,
    onlyItem,
    MAX_COMPACT_COMPANIONS,
    MAX_VERTICAL_COMPANIONS,
    corners,
    mostUrgent,
    ringing,
    sliderPercent,
    type Card,
    type Edge,
    type Section,
    type Style,
  } from './content'

  let {
    cards,
    start,
    upwards,
    foldedWidth,
    duration,
    gap,
    allSections,
    note,
    moduleUis,
    notice,
    spotlight,
    bubble,
    bubbleWidth = $bindable(),
    ontextreadtime,
    edge,
    style,
    expanded,
    movable,
    onclick,
    onaction,
    onenter,
    onleave,
    onpointerdown,
    onpointermove,
    onpointerup,
    onmenu,
  }: {
    // The notch, then the other cards of the separate layout (DF-0011).
    cards: Card[]
    // Where the stack begins along the edge, from App.svelte (it also sets
    // the window region from the same numbers).
    start: number
    // Left / right edges: the notch at the bottom of the stack, the other
    // cards above it (no room below).
    upwards: boolean
    // Width of the closed notch: the other cards open from it, like the notch.
    foldedWidth: number
    // Opening / closing animation, in ms.
    duration: number
    // Space between two cards.
    gap: number
    // The notch's modules (pinned ones apart), the compact notch picks one.
    allSections: Section[]
    note: string | null
    moduleUis: ModuleUis
    notice: string | null
    // A module showing its value for a moment, even when quiet (DF-0013).
    spotlight: string | null
    // Left / right edges: the closed notch shows that value in a small
    // horizontal bubble (DF-0027), as wide as `bubbleWidth`, measured here.
    bubble: boolean
    bubbleWidth?: number
    // How long a module's compact text needs to be read once (Marquee).
    ontextreadtime: (module: string, ms: number) => void
    edge: Edge
    style: Style
    expanded: boolean
    movable: boolean
    // A click on a module (its id); null outside any module.
    // A click on a module, on one of its items if the click was on one.
    onclick: (module: string | null, item?: string) => void
    // A button of an item: `itemId` is `<module id>:<item id>`.
    onaction: (itemId: string, actionId: string) => void
    onenter: () => void
    onleave: () => void
    onpointerdown: (e: PointerEvent) => void
    onpointermove: () => void
    onpointerup: () => void
    // Right click on a module (its id).
    onmenu: (module: string, item?: string) => void
  } = $props()

  // The compact notch shows a single module (DF-0011). Only some items have
  // a dot (a state, e.g. a Claude Code session), others just show text.
  const shown = $derived(compactSection(allSections))
  const top = $derived(shown && mostUrgent(shown.items))
  const dotted = $derived(
    compactMarks(
      shown?.items ?? [],
      edge === 'left' || edge === 'right' ? MAX_VERTICAL_COMPANIONS : MAX_COMPACT_COMPANIONS,
    ),
  )
  // The item's own icon (the volume's level), else the module's.
  const ShownIcon = $derived(
    top?.icon ? icons[top.icon] : shown ? iconOf(shown.module) : null,
  )
  const compactLabel = $derived(top ? (top.compact?.label ?? top.label) : '')
  const compactTitle = $derived(top ? (top.compact ? top.compact.title : top.title) : '')
  // A spotlight shows the text of a quiet item, if it has one: an item with
  // nothing to say (a media module with nothing playing) keeps its icon.
  const quiet = $derived(!!top?.quiet && (shown?.module !== spotlight || !compactLabel))

  function iconOf(module: string) {
    return moduleUis[module]?.icon ?? Puzzle
  }
  const vertical = $derived(edge === 'left' || edge === 'right')
  // Thin and vertical: the closed notch on the left / right edges, unless it
  // is a bubble.
  const thin = $derived(vertical && !expanded && !notice && !bubble)

  let rowWidth = $state(0)
  $effect(() => {
    if (bubble && rowWidth > 0) bubbleWidth = Math.ceil(rowWidth) + 32
  })

  // The cards are stacked: towards the inside of the screen on the top and
  // bottom edges, down along the edge on the left and right ones.
  const notch = $derived(cards[0])
  const stackWidth = $derived(Math.max(...cards.map((c) => c.width)))
  const stackHeight = $derived(
    cards.reduce((sum, c) => sum + c.height, 0) + gap * (cards.length - 1),
  )
  // Each card has its final place from the start, from the attached side:
  // it unfolds there instead of being pushed by the cards before it.
  const offsets = $derived(
    cards.map((_, i) => cards.slice(0, i).reduce((sum, c) => sum + c.height + gap, 0)),
  )
  // A card opens like the notch: from the closed notch's width and from
  // nothing in height, in its place (from the attached side).
  function unfold(node: HTMLElement, { delay, from }: { delay: number; from: number }) {
    const width = node.offsetWidth
    const height = node.offsetHeight
    return {
      delay,
      duration,
      easing: cubicOut,
      css: (t: number) =>
        `overflow: hidden; width: ${from + (width - from) * t}px; height: ${height * t}px`,
    }
  }

  // The cards' offsets count from the stack's bottom: on the bottom edge, and
  // upwards on the left / right ones.
  const fromBottom = $derived(edge === 'bottom' || (vertical && upwards))

  const position = $derived(
    {
      top: `left: ${start}px; top: 0`,
      bottom: `left: ${start}px; bottom: 0`,
      left: `top: ${start}px; left: 0`,
      right: `top: ${start}px; right: 0`,
    }[edge],
  )

  const notchCorners = $derived(corners(edge, notch.radius, style === 'notch'))
  // A ringing module pulses the shape that shows it (DF-0009).
  const notchRinging = $derived(
    expanded ? ringing(notch.sections) : !!shown && ringing([shown]),
  )
</script>

<!-- The window is larger than the notch: the stack of cards is glued to the
     attached side, it then grows towards the inside of the screen. Hovering
     the stack, gaps between cards included, keeps the notch open. -->
<div class="frame" style:--duration="{duration}ms">
  <div
    class="stack edge-{edge}"
    class:upwards={vertical && upwards}
    class:movable
    role="presentation"
    style={position}
    style:width="{stackWidth}px"
    style:height="{stackHeight}px"
    onmouseenter={onenter}
    onmouseleave={onleave}
  >
    <!-- Not a <button>: it holds the items' buttons. The window never takes
         the keyboard focus, so there is no keyboard access to provide. -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      role="button"
      tabindex="-1"
      class="notch"
      class:vertical={thin}
      class:movable
      class:ringing={notchRinging}
      style:width="{notch.width}px"
      style:height="{notch.height}px"
      style:border-radius={notchCorners}
      onclick={() => !expanded && onclick(shown?.module ?? null)}
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
      oncontextmenu={(e) => {
        // Open, each module's part has its own menu (Sections).
        // Closed: the module's only item if it has one (a companion).
        if (!expanded && shown) onmenu(shown.module, onlyItem(shown.items))
        e.preventDefault()
      }}
    >
      {#if notice}
        <span class="row compact"><span class="label">{notice}</span></span>
      {:else if expanded}
        <Sections list={notch.sections} {note} {moduleUis} {onaction} {onmenu} onpick={onclick} />
      {:else if thin}
        <!-- Thin vertical notch: the shown module's dots, else its picture
             or icon; details on hover. -->
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
        {:else if top && top.image !== null && shown}
          <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed} />
        {:else if ShownIcon}
          <ModuleIcon icon={ShownIcon} item={top} {onaction} />
        {/if}
      {:else if top}
        <span
          class="row compact"
          class:bubble
          class:slider={top.meter && !!top.slider && !quiet}
          bind:offsetWidth={rowWidth}
        >
          {#if dotted.shown.length > 0}
            <span class="dots">
              {#each dotted.shown as item (item.id)}
                <Dot {item} compact />
              {/each}
              {#if dotted.hidden > 0}<More count={dotted.hidden} />{/if}
            </span>
          {/if}
          {#if quiet && top.image !== null && shown}
            <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed} />
          {:else if quiet && ShownIcon}
            <ModuleIcon icon={ShownIcon} item={top} {onaction} />
          {:else if top.meter && top.slider && ShownIcon}
            <!-- A slider (the volume): its icon, its value as a bar across
                 the notch, then as a number; no label nor title. Its compact
                 text instead of the bar and number, if it has one (muted). -->
            <ModuleIcon icon={ShownIcon} item={top} {onaction} />
            {#if top.compact}
              <!-- Faint, like a muted icon: discreet. -->
              <span class="label muted">{top.compact.label}</span>
            {:else}
              <span class="meter">
                <span style:width="{sliderPercent(top.slider)}%"></span>
              </span>
              <span class="value">{Math.round(top.slider.value)}</span>
            {/if}
          {:else}
            {#if top.slider}
              <!-- Another slider's value as a small bar (a track's progress). -->
              <span class="meter">
                <span style:width="{sliderPercent(top.slider)}%"></span>
              </span>
            {/if}
            {#if top.image !== null && shown}
              <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed} />
            {/if}
            {#if compactLabel}
              <Marquee
                text={compactLabel}
                scroll={top.scroll}
                dimmed={top.dimmed}
                onturn={(ms) => shown && ontextreadtime(shown.module, ms)}
              />
            {/if}
          {/if}
          {#if compactTitle && !top.meter}<span class="muted">{compactTitle}</span>{/if}
        </span>
      {/if}
    </div>
    <!-- Separate layout: the other modules, one card each, rounded all round.
         Each unfolds in its place from the attached side, one after the
         other, once the notch has started opening. -->
    {#each cards.slice(1) as card, i (card.sections[0]?.module)}
      <div
        role="button"
        tabindex="-1"
        class="notch card"
        class:ringing={ringing(card.sections)}
        style:top={fromBottom ? null : `${offsets[i + 1]}px`}
        style:bottom={fromBottom ? `${offsets[i + 1]}px` : null}
        style:width="{card.width}px"
        style:height="{card.height}px"
        style:border-radius="{card.radius}px"
        in:unfold={{ delay: Math.round(duration * 0.28) * (i + 1), from: foldedWidth }}
        out:fade={{ duration: Math.round(duration / 2) }}
      >
        <Sections
          list={card.sections}
          height={card.height}
          {moduleUis}
          {onaction}
          {onmenu}
          onpick={onclick}
        />
      </div>
    {/each}
  </div>
</div>

<style>
  .frame {
    position: relative;
    height: 100%;
  }

  /* The notch, then the other cards of the separate layout (DF-0011), each
     at its own place. It grows with the notch, so they stay centered on it. */
  .stack {
    position: absolute;
    transition:
      left var(--duration) ease,
      top var(--duration) ease,
      width var(--duration) ease,
      height var(--duration) ease;
  }

  .notch {
    all: unset;
    position: absolute;
    box-sizing: border-box;
    display: block;
    overflow: hidden;
    padding: 0 16px;
    background: var(--notch-fill);
    outline: 1px solid var(--notch-border);
    outline-offset: -1px;
    transition:
      width var(--duration) ease,
      height var(--duration) ease,
      border-radius var(--duration) ease;
  }

  /* Move mode: a grab cursor and a light outline say the notch can be dragged.
     No animation: the shape must follow the cursor at once. */
  .stack.movable,
  .notch.movable {
    transition: none;
  }

  /* Against the attached side; centered across it on the top and bottom
     edges, glued to it on the left and right ones. */
  .edge-top > .notch {
    top: 0;
  }

  .edge-bottom > .notch {
    bottom: 0;
  }

  .edge-left > .notch,
  .edge-right > .notch {
    top: 0;
  }

  .stack.upwards > .notch {
    top: auto;
    bottom: 0;
  }

  .edge-top > .notch,
  .edge-bottom > .notch {
    left: 50%;
    transform: translateX(-50%);
  }

  .edge-left > .notch {
    left: 0;
  }

  .edge-right > .notch {
    right: 0;
  }

  .notch.movable {
    cursor: grab;
    box-shadow: inset 0 0 0 1px var(--notch-outline);
  }

  .notch.movable:active {
    cursor: grabbing;
  }

  .notch.vertical {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .compact {
    height: 34px;
    justify-content: center;
  }

  /* At its natural width, measured to size the bubble. */
  .row.bubble {
    width: max-content;
  }

  .meter {
    flex-shrink: 0;
    width: 64px;
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--notch-track);
  }

  /* A slider across most of the closed notch: the bar takes the room left. */
  .row.slider:not(.bubble) {
    width: 95%;
    margin: 0 auto;
    gap: 10px;
  }

  .row.slider:not(.bubble) .meter {
    flex: 1;
  }

  /* Same width for every value, the bar does not jitter; against the right
     end, so no gap after the number. */
  .value {
    min-width: 3ch;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .meter > span {
    display: block;
    height: 100%;
    background: var(--notch-fg);
    transition: width 150ms ease;
  }
</style>
