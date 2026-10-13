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
    flareStyles,
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
    flare,
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
    // Radius of its outward curves where it meets the edge (DF-0028), 0 for
    // none.
    flare: number
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

  // A playing track shows in full for a moment (a new track, playing again,
  // a spotlight), then fades like a paused one: still there, no longer
  // catching the eye. Longer if its text takes longer to scroll once.
  // The compact text that has scrolled once already (DF-0033): kept here,
  // the text itself is redrawn when the notch opens and closes.
  let scrolledText = $state<string | null>(null)

  const SETTLE_MS = 5000
  let settled = $state(false)
  let settleTimer: ReturnType<typeof setTimeout> | undefined
  // When the full colours started, and when the text will have scrolled once.
  let shownSince = 0
  let readUntil = 0
  const settleKey = $derived(
    top?.activity === 'music' && !top.dimmed ? `${shown?.module}\n${compactLabel}` : null,
  )
  function settleLater() {
    clearTimeout(settleTimer)
    if (settleKey === null) return
    const at = Math.max(shownSince + SETTLE_MS, readUntil)
    settleTimer = setTimeout(() => (settled = true), at - Date.now())
  }
  function showInFull() {
    settled = false
    shownSince = Date.now()
    settleLater()
  }
  $effect(() => {
    void settleKey
    showInFull()
    return () => clearTimeout(settleTimer)
  })
  // A spotlight starting shows it in full again; its end changes nothing.
  const spotlit = $derived(!!shown && shown.module === spotlight)
  $effect(() => {
    if (spotlit) showInFull()
  })

  // The closed notch's slider that is not a level (a track's progress,
  // DF-0033), on the top and bottom edges: drawn along the outline.
  const progress = $derived(
    !notice &&
      !expanded &&
      edge !== 'left' &&
      edge !== 'right' &&
      !!top?.slider &&
      top.outline &&
      !top.meter &&
      !quiet
      ? top.slider
      : null,
  )
  // Its path, half a line inside the shape: down the left side, along the
  // bottom, up the right side, following the rounded corners. Drawn for the
  // top edge, flipped for the bottom one. It comes out from under the
  // outward curves (DF-0028): it starts where they meet the sides, the
  // notch keeping its curves whole. The pill's top corners are left out.
  const progressPath = $derived.by(() => {
    const { width: w, height: h, radius: r } = notch
    // The line reaches half a pixel past the edge, cut there with the notch:
    // it covers the notch's own smoothed edge, no dark rim outside it.
    const s = 0.75
    const a = Math.max(0, r - s)
    const from = style === 'notch' ? flare : r
    return [
      `M ${s} ${from}`,
      `L ${s} ${h - r}`,
      `A ${a} ${a} 0 0 0 ${r} ${h - s}`,
      `L ${w - r} ${h - s}`,
      `A ${a} ${a} 0 0 0 ${w - s} ${h - r}`,
      `L ${w - s} ${from}`,
    ].join(' ')
  })

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
  // Each card starts unfolding a little after the one before. The last one
  // starts STAGGER of the duration after the notch with two cards, and the
  // number of steps to the power STAGGER_GROWTH times that with more: a
  // longer list unfolds quicker, step by step, like Apple's lists (DF-0029).
  const STAGGER = 0.28
  const STAGGER_GROWTH = 0.7
  const stagger = $derived.by(() => {
    const steps = cards.length - 1
    const last = duration * STAGGER * steps ** STAGGER_GROWTH
    return steps > 0 ? last / steps : 0
  })

  // A card opens like the notch: from the closed notch's width and from
  // nothing in height, in its place (from the attached side), fading in so
  // that the cards are seen arriving one after the other.
  function unfold(node: HTMLElement, { delay, from }: { delay: number; from: number }) {
    const width = node.offsetWidth
    const height = node.offsetHeight
    return {
      delay,
      duration,
      easing: cubicOut,
      css: (t: number) =>
        `overflow: hidden; width: ${from + (width - from) * t}px; height: ${height * t}px; opacity: ${t}`,
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
  // The notch's outward curves (DF-0028), placed in the stack: it moves and
  // grows with the notch. On the left / right edges with other cards along
  // the edge, only the one away from them.
  const flares = $derived.by(() => {
    if (flare <= 0) return []
    const both = flareStyles(edge, flare)
    if (!vertical || cards.length === 1) return both
    return upwards ? [both[1]] : [both[0]]
  })
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
    {#each flares as style, i (i)}<span class="flare" {style}></span>{/each}
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
          <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed || settled} />
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
            <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed || settled} />
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
            {#if top.image !== null && shown}
              <Cover src={top.image} icon={iconOf(shown.module)} dimmed={top.dimmed || settled} />
            {/if}
            {#if compactLabel}
              <Marquee
                text={compactLabel}
                scroll={top.scroll && scrolledText !== compactLabel}
                dimmed={top.dimmed || settled}
                onturn={(ms) => {
                  if (shown) ontextreadtime(shown.module, ms)
                  readUntil = Date.now() + ms
                  if (!settled) settleLater()
                }}
                onscrolled={() => (scrolledText = compactLabel)}
              />
            {/if}
          {/if}
          {#if compactTitle && !top.meter}<span class="muted">{compactTitle}</span>{/if}
        </span>
      {/if}
      {#if progress}
        <!-- A track's progress (DF-0033): a line along the closed notch's
             outline, down its left side, along the side away from the edge,
             up its right side, out from under the outward curves. Fades in
             once the notch has folded back, gone at once when it opens. -->
        <svg
          in:fade={{ delay: duration, duration }}
          out:fade={{ duration: 0 }}
          class="progress"
          class:flipped={edge === 'bottom'}
          class:faded={!!top?.dimmed || settled}
          width={notch.width}
          height={notch.height}
          aria-hidden="true"
        >
          <path class="track" d={progressPath} pathLength="100" />
          <path
            class="done"
            d={progressPath}
            pathLength="100"
            stroke-dasharray="100"
            stroke-dashoffset={100 - sliderPercent(progress)}
          />
        </svg>
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
        in:unfold={{ delay: Math.round(stagger * (i + 1)), from: foldedWidth }}
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

  /* Over the whole closed notch, cut by its rounded corners; drawn for the
     top edge, turned upside down on the bottom one, where the outline's
     open side is the top. */
  .progress {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    fill: none;
    stroke-width: 2.5;
  }

  .progress.flipped {
    transform: scaleY(-1);
  }

  /* The text colour, see-through: right on any notch colour or opacity.
     Faded (paused, settled), its colour fades, not the whole line's
     opacity, which would let the notch's edge show in the corners. */
  .progress .track {
    stroke: color-mix(in srgb, var(--notch-fg) 20%, transparent);
  }

  .progress .done {
    stroke: var(--notch-fg);
    transition:
      stroke-dashoffset 150ms ease,
      stroke calc(var(--duration, 250ms) * 3) ease;
  }

  .progress.faded .done {
    stroke: color-mix(in srgb, var(--notch-fg) 50%, transparent);
  }
</style>
