<script lang="ts">
  import Puzzle from '@lucide/svelte/icons/puzzle'
  import type { ModuleUis } from '../modules'
  import Cover from './Cover.svelte'
  import Dot from './Dot.svelte'
  import ModuleIcon from './ModuleIcon.svelte'
  import { rowWidth } from './companions'
  import { sliderPercent, type Item, type Section } from './content'
  import { filled, icons } from './icons'

  // The modules shown in an open card or pin, one part each, its icon first
  // (DF-0011).
  let {
    list,
    note = null,
    height = null,
    moduleUis,
    onaction,
    onmenu,
    onpick,
  }: {
    list: Section[]
    // Shown when the list is empty (no module enabled).
    note?: string | null
    // The card's final height, null to follow the card. A fixed height keeps
    // the rows still while the card unfolds over them.
    height?: number | null
    moduleUis: ModuleUis
    // A button of an item: `itemId` is `<module id>:<item id>`.
    onaction: (itemId: string, actionId: string) => void
    // Right click on a module's part (its id), on one of its items (its
    // `<module id>:<item id>`) if the click was on one.
    onmenu?: (module: string, item?: string) => void
    // A click on a module's part (its id): it acknowledges that module, or
    // only the clicked item (its `<module id>:<item id>`) if the click was on
    // one.
    onpick?: (module: string, item?: string) => void
  } = $props()

  // The item under a click: `<module id>:<item id>`, if it was on one.
  function itemAt(e: MouseEvent) {
    return (e.target as Element).closest<HTMLElement>('[data-item]')?.dataset.item
  }

  function iconOf(module: string) {
    return moduleUis[module]?.icon ?? Puzzle
  }

  // While a slider is held, it shows the cursor's value, not the module's:
  // the module's own updates would make it jump back under the cursor.
  let held = $state<Record<string, number>>({})
</script>

{#snippet slider(item: Item)}
  {@const slider = item.slider!}
  {@const value = held[item.id] ?? slider.value}
  <input
    type="range"
    class="slider"
    aria-label={item.title}
    min={slider.min}
    max={slider.max}
    step={slider.step}
    {value}
    disabled={slider.readonly}
    style:--fill="{sliderPercent(slider, value)}%"
    onpointerdown={() => (held[item.id] = slider.value)}
    oninput={(e) => {
      const next = Number(e.currentTarget.value)
      held[item.id] = next
      onaction(item.id, `set:${next}`)
    }}
    onpointerup={() => delete held[item.id]}
    onpointercancel={() => delete held[item.id]}
    onclick={(e) => e.stopPropagation()}
  />
{/snippet}

{#snippet actions(item: Item, size: number)}
  <span class="actions">
    {#each item.actions as action (action.id)}
      {@const ButtonIcon = action.icon ? icons[action.icon] : null}
      <!-- Filled icons: a thin stroke (it rounds their corners and draws the
           skip icons' bar) and smaller, or they look heavy; an even size in
           the even-sized buttons, so they sit on whole pixels, centered. -->
      {@const full = action.icon !== null && filled.has(action.icon)}
      <button
        class="action"
        class:text={!ButtonIcon}
        title={action.label}
        aria-label={action.label}
        onclick={(e) => {
          // Not an acknowledgement of the whole notch.
          e.stopPropagation()
          onaction(item.id, action.id)
        }}
      >
        {#if ButtonIcon}
          <ButtonIcon
            size={full ? size - 3 : size}
            fill={full ? 'currentColor' : 'none'}
            strokeWidth={full ? 1.5 : 2.5}
          />
        {:else}
          {action.label}
        {/if}
      </button>
    {/each}
  </span>
{/snippet}

<div
  class="list"
  class:centered={list.length === 0}
  style:height={height === null ? null : `${height}px`}
>
  {#each list as section (section.module)}
    {@const Icon = iconOf(section.module)}
    {@const dottedRows = section.items.some((i) => i.dot)}
    <!-- With a companion in the module, every dot gets its width (DF-0020). -->
    {@const companionSize = section.items.find((i) => i.companion)?.companion?.size}
    <!-- A featured item (media player) carries its own picture: no icon. -->
    {@const featured = section.items.some((i) => i.layout === 'featured')}
    <!-- An item whose icon is a button (the volume's mute) lends it to the
         module: its icon stands first, and acts. -->
    {@const lead = section.items.find((i) => i.iconAction)}
    <!-- The notch window never takes the keyboard focus (see Notch.svelte). -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div
      class="section"
      role="group"
      onclick={(e) => onpick?.(section.module, itemAt(e))}
      oncontextmenu={(e) => {
        // In a pin, the pin itself shows the menu.
        if (!onmenu) return
        e.preventDefault()
        e.stopPropagation()
        onmenu(section.module, itemAt(e))
      }}
    >
      {#if !featured}
        <ModuleIcon
          icon={lead?.icon ? icons[lead.icon] : Icon}
          item={lead ?? null}
          {onaction}
        />
      {/if}
      <ul class="rows">
        {#each section.items as item (item.id)}
          {#if item.layout === 'featured'}
            <!-- A media player: the picture, then everything stacked. -->
            <li class="featured" class:dimmed={item.dimmed} data-item={item.id}>
              <Cover src={item.image ?? ''} icon={Icon} />
              <div class="stack">
                {#if item.title}<span class="title">{item.title}</span>{/if}
                {#if item.label}<span class="label lead">{item.label}</span>{/if}
                {#if item.slider || item.detail}
                  <span class="progress">
                    {#if item.slider}{@render slider(item)}{/if}
                    {#if item.detail}<span class="label value">{item.detail}</span>{/if}
                  </span>
                {/if}
                {#if item.actions.length > 0}{@render actions(item, 15)}{/if}
              </div>
            </li>
          {:else}
            <!-- An item with a state (a session) can be picked alone: its
                 row lights up under the cursor. -->
            <li class="row" class:dimmed={item.dimmed} class:pickable={item.dot} data-item={item.id}>
              {#if item.image !== null}<Cover src={item.image} icon={Icon} />{/if}
              {#if item.dot || dottedRows}
                <span
                  class="mark"
                  style:width={companionSize ? `${rowWidth(companionSize)}px` : null}
                >
                  {#if item.dot}
                    <Dot {item} />
                  {:else}
                    <!-- Keeps the texts aligned with the dotted rows. -->
                    <span class="dot blank"></span>
                  {/if}
                </span>
              {/if}
              {#if item.title}<span class="title">{item.title}</span>{/if}
              {#if item.slider}{@render slider(item)}{/if}
              <span class="label" class:lead={!item.title && !item.slider} class:value={!!item.slider}>
                {item.label}{item.detail ? ` · ${item.detail}` : ''}
              </span>
              {#if item.actions.length > 0}{@render actions(item, 13)}{/if}
            </li>
          {/if}
        {/each}
        <!-- A module without items still says how it is (ADR-0009). -->
        {#if section.note}
          <li class="row"><span class="label muted note">{section.note}</span></li>
        {/if}
      </ul>
    </div>
  {:else}
    {#if note}
      <span class="row"><span class="label muted note">{note}</span></span>
    {/if}
  {/each}
</div>

<style>
  /* Fills the open card and centers its rows: a vertical notch never gets
     shorter than its compact shape, which leaves room around a short list.
     Past the window height it scrolls ("safe": the top stays reachable).
     Scrolling clips the sides too: it reaches 8 px into the card's padding,
     so a picked row's background is not cut. */
  .list {
    box-sizing: border-box;
    height: 100%;
    margin-inline: -8px;
    padding: 8px 8px 10px;
    display: grid;
    align-content: safe center;
    gap: 2px;
    overflow-y: auto;
    scrollbar-width: none;
  }

  .section {
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  /* Between two modules: a thin line. Its height (6 + 1 + 8 px) is
     SEPARATOR_HEIGHT in App.svelte. */
  .section + .section {
    margin-top: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--notch-line);
  }

  .rows {
    flex: 1;
    min-width: 0;
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    height: 24px;
  }

  /* The background reaches a little past the row's texts: 8 px on the
     right, 3 px on the left, so it keeps clear of the module's icon. */
  .row.pickable {
    margin-inline: -3px -8px;
    padding-inline: 3px 8px;
    border-radius: 6px;
    transition: background-color 150ms ease;
  }

  .row.pickable:hover {
    background: var(--notch-button);
  }

  /* The dot or companion before the texts, centered in its place. */
  .mark {
    display: flex;
    justify-content: center;
    flex-shrink: 0;
  }

  .centered .row {
    justify-content: center;
  }

  .title {
    flex: 0 1 auto;
    font-weight: 600;
  }

  .label {
    margin-left: auto;
    opacity: 0.75;
  }

  .note,
  .lead {
    margin-left: 0;
  }

  /* A slider fills the row; its value keeps a steady width after it. */
  .slider {
    flex: 1;
    min-width: 60px;
    height: 4px;
    margin: 0;
    appearance: none;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--notch-fg) var(--fill),
      var(--notch-track) var(--fill)
    );
    cursor: pointer;
    outline: none;
  }

  /* Read-only: a plain bar, no handle. */
  .slider:disabled {
    cursor: default;
  }

  .slider:disabled::-webkit-slider-thumb {
    visibility: hidden;
  }

  .slider::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--notch-fg);
  }

  .label.value {
    margin-left: 0;
    min-width: 40px;
    text-align: right;
  }

  .actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .action {
    all: unset;
    display: grid;
    place-items: center;
    min-width: 20px;
    height: 20px;
    border-radius: 6px;
    background: var(--notch-button);
    color: var(--notch-fg);
    cursor: pointer;
    transition: background-color 150ms ease;
  }

  .action.text {
    padding: 0 7px;
    font-size: 11px;
  }

  .action:hover {
    background: var(--notch-button-hover);
  }

  /* A media player: its height (84 px + the rows' 2 px gap) is
     FEATURED_HEIGHT in App.svelte. */
  .featured {
    display: flex;
    gap: 12px;
    min-width: 0;
    height: 84px;
    align-items: center;
  }

  .featured > :global(.cover) {
    width: 76px;
    height: 76px;
    border-radius: 8px;
  }

  .featured .stack {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .featured .title,
  .featured .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .featured .progress {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 16px;
  }

  .featured .actions {
    justify-content: center;
    gap: 10px;
  }

  /* Square with rounded corners, like the other buttons, only larger. */
  .featured .action {
    min-width: 28px;
    height: 28px;
    border-radius: 8px;
  }

  /* Paused: only the picture fades a little. Opened, the item is there to
     be read and used: its texts stay clear (DF-0033). */
  .featured.dimmed > :global(.cover) {
    opacity: 0.7;
  }
</style>
