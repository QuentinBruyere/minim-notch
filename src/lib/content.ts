import type { SoundKind } from './sound'
import type { Appearance } from './theme'

// Mirrors `Tone` in src-tauri/src/module.rs: colour, urgency and sound of an item.
export type Tone = 'neutral' | 'active' | 'attention' | 'question' | 'success' | 'error'

// Mirrors `Icon` in src-tauri/src/module.rs
export type Icon =
  | 'play'
  | 'pause'
  | 'reset'
  | 'volume_low'
  | 'volume_medium'
  | 'volume_high'
  | 'muted'
  | 'previous'
  | 'next'

// Mirrors `Action` in src-tauri/src/module.rs: a button at the end of a row.
export interface Action {
  id: string
  // Tooltip of an icon button, text of a button without icon ("+1 min").
  label: string
  icon: Icon | null
}

// Mirrors `Slider` in src-tauri/src/module.rs: moving it sends `set:<value>`.
export interface Slider {
  value: number
  min: number
  max: number
  step: number
  // Shows the value only (a progress the app does not let seek).
  readonly: boolean
}

// Where a slider's value sits, from 0 to 100.
export function sliderPercent(slider: Slider, value = slider.value): number {
  const span = slider.max - slider.min
  return span > 0 ? ((value - slider.min) / span) * 100 : 0
}

// Mirrors `Item` in src-tauri/src/module.rs: one line of the notch.
export interface Item {
  // `<module id>:<item id>`
  id: string
  title: string
  label: string
  detail: string | null
  tone: Tone
  // Shows a dot in the tone's colour; text-only items (the time) have none.
  dot: boolean
  // Drawn instead of the dot (DF-0020): a companion, see lib/companions.
  // `color`: worn while working, darker at rest (`--companion-<color>` in
  // app.css); null = always the state's. `reactive`: its mood follows the
  // notch and the mouse, not the tone (DF-0024).
  companion: {
    id: string
    size: CompanionSize
    color: string | null
    dimmed: boolean
    reactive: boolean
  } | null
  // What it is busy with (DF-0024): music playing, something running.
  activity: 'music' | 'running' | null
  // Its compact text is worth seeing all along (a stopwatch or timer in use):
  // on the left / right edges, the thin views show it in a bubble (DF-0027).
  bubble: boolean
  // Short lines stacked in the thin views of the left / right edges, instead
  // of the icon (the clock's hours over minutes, DF-0027).
  stack: string[]
  // Compact views: its slider as a level, icon, bar and number (the volume),
  // or its compact text instead (muted).
  meter: boolean
  actions: Action[]
  // A slider in the open notch, between the title and the label.
  slider: Slider | null
  // Nothing worth showing yet: compact views show the module's icon instead.
  // Nothing worth showing: compact views show the image, else the module's
  // icon; a spotlight shows the label for a moment.
  quiet: boolean
  // Replaces the module's icon in the compact views (the volume's level).
  icon: Icon | null
  // Makes the item's icon a button (the volume's mute), wherever it shows.
  iconAction: Action | null
  // Rings (a timer whose time is up): an alarm repeats and its shape pulses.
  ringing: boolean
  // The texts of the compact views (closed notch, pin), if not the label
  // and title (the clock's own choice).
  compact: { label: string; title: string } | null
  // A small square picture before the texts (an album cover), a data: URL;
  // empty, or if it fails, the module's icon stands in (Cover.svelte).
  image: string | null
  // Shown faded (a paused track).
  dimmed: boolean
  // Compact views: a text too long scrolls instead of being cut (Marquee).
  scroll: boolean
  // Mirrors `Layout` in src-tauri/src/module.rs: one line, or a large
  // picture with the texts, slider and buttons stacked on its right.
  layout: 'row' | 'featured'
}

// Mirrors `Section` in src-tauri/src/module.rs: one module's part of the notch.
export interface Section {
  // Module id, picks the section's icon.
  module: string
  items: Item[]
  // The module's placeholder when it has no item, shown in the open notch.
  note: string | null
  // Pinned: shown in its own mini-notch on this side of the notch (DF-0012).
  pin: Side | null
  // Its pin opens when hovered; else it keeps its small shape (DF-0024).
  pinOpens: boolean
}

// Side of a pin, mirrors `Side` in src-tauri/src/module.rs. On the left and
// right edges of the screen: above and below the notch.
export type Side = 'left' | 'right'

// A rectangle in logical pixels, relative to the window.
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

// A visible shape for the window region, mirrors `notch::Shape` in Rust.
export interface Shape extends Rect {
  radius: number
  // Glued to the screen edge: square corners on that side.
  attached: boolean
  // Radius of the outward curves where it meets the edge (DF-0028); 0: none.
  flare: number
}

// Where an attached shape's two outward curves go (DF-0028), as inline
// styles for `.flare` elements inside its box: a small square on each side
// against the edge, the circle cut out at its corner away from both.
export function flareStyles(edge: Edge, r: number): string[] {
  const spots: Record<Edge, [string, string][]> = {
    top: [
      [`left: -${r}px; top: 0`, '0 100%'],
      [`right: -${r}px; top: 0`, '100% 100%'],
    ],
    bottom: [
      [`left: -${r}px; bottom: 0`, '0 0'],
      [`right: -${r}px; bottom: 0`, '100% 0'],
    ],
    left: [
      [`left: 0; top: -${r}px`, '100% 0'],
      [`left: 0; bottom: -${r}px`, '100% 100%'],
    ],
    right: [
      [`right: 0; top: -${r}px`, '0 0'],
      [`right: 0; bottom: -${r}px`, '0 100%'],
    ],
  }
  return spots[edge].map(([place, at]) => `${place}; width: ${r}px; height: ${r}px; --r: ${r}px; --at: ${at}`)
}

// The box around two places of a moving shape: every place in between too.
// Its corners are the rounder one's, so neither shape pokes out of them.
export function around(a: Shape, b: Shape): Shape {
  const x = Math.min(a.x, b.x)
  const y = Math.min(a.y, b.y)
  return {
    x,
    y,
    width: Math.max(a.x + a.width, b.x + b.width) - x,
    height: Math.max(a.y + a.height, b.y + b.height) - y,
    radius: Math.min(a.radius, b.radius),
    attached: a.attached && b.attached,
    flare: Math.max(a.flare, b.flare),
  }
}

// Mirrors `Content` in src-tauri/src/module.rs: sections in the user's order.
export interface Content {
  sections: Section[]
  // Shown in the open notch when no module is enabled.
  note: string | null
}

export function allItems(content: Content): Item[] {
  return content.sections.flatMap((s) => s.items)
}

// Whether one of these modules rings: the shape showing them pulses.
export function ringing(sections: Section[]): boolean {
  return sections.some((s) => s.items.some((i) => i.ringing))
}

// Screen edge the notch is attached to, mirrors `Edge` in src-tauri/src/placement.rs
export type Edge = 'top' | 'bottom' | 'left' | 'right'

// Notch glued to the edge or pill detached from it, mirrors `Style` in
// src-tauri/src/placement.rs
export type Style = 'notch' | 'pill'

// Modules all in the notch, or one card each, mirrors `ModuleLayout` in
// src-tauri/src/config.rs (DF-0011)
export type ModuleLayout = 'joined' | 'separate'

// A drawn card: the notch first, then, in the separate layout, one card per
// other module. Sizes in logical pixels, mirrored by `notch::Card` in Rust.
export interface Card {
  width: number
  height: number
  radius: number
  // Modules shown in it when the notch is open.
  sections: Section[]
}

// Mirrors `NotchSpeed` in src-tauri/src/config.rs
// Mirrors `CompanionSize` in src-tauri/src/config.rs (DF-0020).
export type CompanionSize = 'small' | 'medium' | 'large'

export type NotchSpeed = 'slow' | 'normal' | 'fast'

// Length of the opening / closing animation, in ms.
export const speedMs: Record<NotchSpeed, number> = { slow: 400, normal: 250, fast: 150 }

// Mirrors `Status` in src-tauri/src/lib.rs
export interface Status {
  soundEnabled: boolean
  edge: Edge
  style: Style
  // Move mode: the notch can be dragged along its edge (DF-0006)
  movable: boolean
  layout: ModuleLayout
  speed: NotchSpeed
  appearance: Appearance
  // The language shown (ADR-0012), the system's already resolved.
  language: string
  // Tucks into the edge when unused, and after how long (DF-0018).
  autoHide: boolean
  // Pins curve outwards where they meet the edge, like the notch (DF-0028).
  pinFlares: boolean
  autoHideDelayMs: number
  // The notch's own color, #rrggbb (DF-0019); null = the theme's.
  notchColor: string | null
  // How opaque its background is, and how much grain on it, 0 to 100.
  notchOpacity: number
  notchGrain: number
  // Center of the compact notch along the edge, from the start of the window,
  // in logical pixels; null = the middle of the window (DF-0006)
  anchor: number | null
}

// CSS corners of a shape: glued to the edge, a notch rounds its inner
// corners only (DF-0006); a pill or a detached card rounds all four.
export function corners(edge: Edge, radius: number, attached: boolean): string {
  const r = radius
  if (!attached) return `${r}px`
  return {
    top: `0 0 ${r}px ${r}px`,
    bottom: `${r}px ${r}px 0 0`,
    left: `0 ${r}px ${r}px 0`,
    right: `${r}px 0 0 ${r}px`,
  }[edge]
}

// Most urgent first: decides what the compact notch shows.
const priority: Tone[] = ['attention', 'error', 'question', 'success', 'active', 'neutral']

export function mostUrgent(items: Item[]): Item | undefined {
  return [...items].sort((a, b) => priority.indexOf(a.tone) - priority.indexOf(b.tone))[0]
}

// How many companions the compact views show at most (DF-0023): a closed
// notch, a pin; fewer in the thin vertical notch.
export const MAX_COMPACT_COMPANIONS = 4
export const MAX_VERTICAL_COMPANIONS = 3

// The items with a dot or companion a compact view shows. With companions
// and more than `max` of them, only the `max` most urgent, in their own
// order (they keep their places); `hidden` says how many are left out.
export function compactMarks(items: Item[], max: number): { shown: Item[]; hidden: number } {
  const marked = items.filter((i) => i.dot)
  if (marked.length <= max || !marked.some((i) => i.companion)) {
    return { shown: marked, hidden: 0 }
  }
  const kept = new Set(
    [...marked]
      .sort((a, b) => priority.indexOf(a.tone) - priority.indexOf(b.tone))
      .slice(0, max),
  )
  return { shown: marked.filter((i) => kept.has(i)), hidden: marked.length - max }
}

// A module's single item, if it has exactly one: what a right click on a
// closed view of it aims at (its own menu entries, e.g. a companion's).
export function onlyItem(items: Item[]): string | undefined {
  return items.length === 1 ? items[0].id : undefined
}

export function toneColor(tone: Tone): string {
  return `var(--tone-${tone})`
}

// Tones that deserve attention: they expand the notch and play a sound (DF-0003).
export function soundFor(tone: Tone): SoundKind | undefined {
  switch (tone) {
    case 'attention':
    case 'question':
      return 'attention'
    case 'success':
      return 'done'
    case 'error':
      return 'error'
    default:
      return undefined
  }
}

// Items that just entered an attention tone, compared to the previous list.
export function newAlerts(previous: Item[], next: Item[]): Item[] {
  const before = new Map(previous.map((i) => [i.id, i.tone]))
  return next.filter((i) => soundFor(i.tone) && before.get(i.id) !== i.tone)
}

// Module of an item, from its `<module id>:<item id>`.
export function moduleOf(item: Item): string {
  return item.id.split(':')[0]
}

// What the compact notch shows (DF-0011): the module with the most urgent
// alert, else the first module with items, in the user's order.
export function compactSection(sections: Section[]): Section | undefined {
  let alert: Section | undefined
  let rank = Infinity
  for (const section of sections) {
    for (const item of section.items) {
      const r = priority.indexOf(item.tone)
      if (soundFor(item.tone) && r < rank) {
        alert = section
        rank = r
      }
    }
  }
  return alert ?? sections.find((s) => s.items.length > 0)
}
