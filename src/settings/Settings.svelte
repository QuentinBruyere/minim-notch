<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app'
  import { invoke } from '@tauri-apps/api/core'
  import { emitTo, listen } from '@tauri-apps/api/event'
  import GripVertical from '@lucide/svelte/icons/grip-vertical'
  import PinIcon from '@lucide/svelte/icons/pin'
  import Puzzle from '@lucide/svelte/icons/puzzle'
  import Palette from '@lucide/svelte/icons/palette'
  import SettingsIcon from '@lucide/svelte/icons/settings'
  import { flip } from 'svelte/animate'
  import { setLanguage, t, type Key } from '../lib/i18n.svelte'
  import { applyAppearance, MAX_GRAIN, type Appearance } from '../lib/theme'
  import type { ModuleUis } from '../modules'
  import CustomCompanions from './CustomCompanions.svelte'
  import Logo from './Logo.svelte'
  import './settings.css'

  let { moduleUis }: { moduleUis: ModuleUis } = $props()

  // Mirrors `Settings` in src-tauri/src/settings.rs
  type Strength = 'soft' | 'medium' | 'strong' | 'veryStrong' | 'impassable'
  type Edge = 'top' | 'bottom' | 'left' | 'right'
  type Style = 'notch' | 'pill'
  type ModuleLayout = 'joined' | 'separate'
  type NotchSpeed = 'slow' | 'normal' | 'fast'
  type PinSide = 'auto' | 'left' | 'right'
  interface Settings {
    soundEnabled: boolean
    hideInFullscreen: boolean
    edge: Edge
    style: Style
    gap: number
    screen: string | null
    screens: { id: string; label: string; primary: boolean }[]
    movable: boolean
    offCenter: boolean
    cursorResistance: boolean
    resistanceStrength: Strength
    resistanceAvailable: boolean
    autostart: boolean
    modules: ModuleInfo[]
    moduleLayout: ModuleLayout
    notchSpeed: NotchSpeed
    appearance: Appearance
    autoHide: boolean
    pinFlares: boolean
    autoHideDelayMs: number
    notchColor: string | null
    notchOpacity: number
    notchGrain: number
    language: string | null
    languages: [string, string][]
    // The language shown, the system's resolved (ADR-0012).
    shownLanguage: string
    configDir: string
    logFile: string
    // The previous run's crash (ADR-0014), until dismissed.
    crash: { at: string; version: string; thread: string; location: string; message: string } | null
    // Updates (ADR-0013); never checked by the development version.
    checkUpdates: boolean
    updatesEnabled: boolean
    update: UpdateStatus
  }

  // Mirrors `updates::Status` in src-tauri/src/updates.rs
  type UpdateStatus =
    | { state: 'idle' | 'checking' | 'upToDate' }
    | { state: 'available'; version: string; notes: string | null }
    | { state: 'downloading'; version: string; percent: number | null }

  // Mirrors `ModuleInfo` in src-tauri/src/settings.rs
  interface ModuleInfo {
    id: string
    name: string
    description: string
    enabled: boolean
    pin: PinSide | null
    pinnable: boolean
    settings: unknown
  }

  // Categories of the left menu (DF-0007).
  type Page = 'general' | 'display' | 'modules' | 'companions' | 'about'
  const pages: { id: Page; key: Key }[] = [
    { id: 'general', key: 'settings.page.general' },
    { id: 'display', key: 'settings.page.display' },
    { id: 'modules', key: 'settings.page.modules' },
    { id: 'companions', key: 'settings.page.companions' },
    { id: 'about', key: 'settings.page.about' },
  ]

  const appearanceChoices: { value: Appearance; key: Key }[] = [
    { value: 'system', key: 'settings.appearance.system' },
    { value: 'light', key: 'settings.appearance.light' },
    { value: 'dark', key: 'settings.appearance.dark' },
  ]

  // Ready-made notch colors (DF-0019); any other through the picker.
  const notchColors = [
    '#1e3a8a',
    '#4c1d95',
    '#9f1239',
    '#166534',
    '#c2410c',
    '#facc15',
    '#ffffff',
  ]

  const speedChoices: { value: NotchSpeed; key: Key }[] = [
    { value: 'slow', key: 'settings.speed.slow' },
    { value: 'normal', key: 'settings.speed.normal' },
    { value: 'fast', key: 'settings.speed.fast' },
  ]

  const layoutChoices: { value: ModuleLayout; key: Key }[] = [
    { value: 'joined', key: 'settings.layout.joined' },
    { value: 'separate', key: 'settings.layout.separate' },
  ]

  const styleChoices: { value: Style; key: Key }[] = [
    { value: 'notch', key: 'settings.style.notch' },
    { value: 'pill', key: 'settings.style.pill' },
  ]

  // Shown while the slider moves, saved when it is released.
  let gap = $state(10)
  $effect(() => {
    if (settings) gap = settings.gap
  })
  // Background opacity and grain, shown on the notch while the sliders move,
  // saved on release.
  let opacity = $state(100)
  let grain = $state(0)
  $effect(() => {
    if (settings) {
      opacity = settings.notchOpacity
      grain = Math.min(settings.notchGrain, MAX_GRAIN)
    }
  })
  function previewNotchLook(look: { opacity: number; grain: number }) {
    void emitTo('notch', 'notch-look-preview', look)
  }

  // Auto-hide delay, in seconds on the slider (DF-0018).
  const HIDE_DELAY_DEFAULT = 1
  const HIDE_DELAY_MAX = 5
  let hideDelay = $state(HIDE_DELAY_DEFAULT)
  $effect(() => {
    if (settings) hideDelay = settings.autoHideDelayMs / 1000
  })
  function saveHideDelay() {
    void run('set_auto_hide_delay', { ms: Math.round(hideDelay * 1000) })
  }

  const edgeChoices: { value: Edge; key: Key }[] = [
    { value: 'top', key: 'settings.edge.top' },
    { value: 'bottom', key: 'settings.edge.bottom' },
    { value: 'left', key: 'settings.edge.left' },
    { value: 'right', key: 'settings.edge.right' },
  ]

  type ResistanceChoice = 'off' | Strength
  const resistanceChoices: { value: ResistanceChoice; key: Key }[] = [
    { value: 'off', key: 'settings.resistance.off' },
    { value: 'soft', key: 'settings.resistance.soft' },
    { value: 'medium', key: 'settings.resistance.medium' },
    { value: 'strong', key: 'settings.resistance.strong' },
    { value: 'veryStrong', key: 'settings.resistance.very_strong' },
    { value: 'impassable', key: 'settings.resistance.impassable' },
  ]

  let settings = $state<Settings | null>(null)
  // "1.5" or "1,5", as the shown language writes it.
  const seconds = $derived(
    new Intl.NumberFormat(settings?.shownLanguage ?? 'en', { maximumFractionDigits: 1 }),
  )
  $effect(() => {
    if (settings) applyAppearance(settings.appearance)
  })
  $effect(() => {
    if (settings) setLanguage(settings.shownLanguage)
  })
  let version = $state('')
  let error = $state<string | null>(null)
  let page = $state<Page>('general')
  // Module whose settings page is open, within the Modules category.
  let openModuleId = $state<string | null>(null)
  const openModule = $derived(
    page === 'modules' ? settings?.modules.find((m) => m.id === openModuleId) : undefined,
  )

  function show(next: Page) {
    page = next
    openModuleId = null
    error = null
  }

  function setModuleEnabled(module: ModuleInfo, enabled: boolean) {
    void run('set_module_enabled', { id: module.id, enabled })
  }

  // Pinned next to the notch (DF-0012); on the left / right edges of the
  // screen, left and right mean above and below.
  const pinSideChoices: { value: PinSide; key: Key }[] = [
    { value: 'auto', key: 'settings.pin_side.auto' },
    { value: 'left', key: 'settings.pin_side.left' },
    { value: 'right', key: 'settings.pin_side.right' },
  ]

  function setModulePin(module: ModuleInfo, pin: PinSide | null) {
    void run('set_module_pin', { id: module.id, pin })
  }

  // Reordering the modules by dragging their handle (DF-0011). The grabbed
  // card follows the pointer, the others slide out of its way; the list
  // itself only changes on release, then the order is saved.
  interface Drag {
    from: number
    to: number
    // How far the grabbed card moved from its place, in px.
    dy: number
    startY: number
    // Layout of the cards when the drag started, in list order.
    tops: number[]
    heights: number[]
    // Room the grabbed card takes: its height plus the gap after it.
    span: number
  }
  let drag = $state<Drag | null>(null)
  // The new order, shown until the saved settings come back.
  let localOrder = $state<string[] | null>(null)
  const cards = $state<Record<string, HTMLElement>>({})
  const orderedModules = $derived.by(() => {
    const modules = settings?.modules ?? []
    if (!localOrder) return modules
    return localOrder.flatMap((id) => modules.find((m) => m.id === id) ?? [])
  })

  // Pointer position in the scrolling area, so scrolling while dragging counts.
  function pointerY(e: PointerEvent, card: HTMLElement) {
    return e.clientY + ((card.offsetParent as HTMLElement | null)?.scrollTop ?? 0)
  }

  function startReorder(e: PointerEvent, id: string) {
    if (e.button !== 0 || drag) return
    const elements = orderedModules.map((m) => cards[m.id])
    const from = orderedModules.findIndex((m) => m.id === id)
    const card = elements[from]
    if (!card || elements.some((el) => !el)) return
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    const gap = parseFloat(getComputedStyle(card.offsetParent ?? card).rowGap) || 0
    drag = {
      from,
      to: from,
      dy: 0,
      startY: pointerY(e, card),
      tops: elements.map((el) => el.offsetTop),
      heights: elements.map((el) => el.offsetHeight),
      span: card.offsetHeight + gap,
    }
  }

  function reorder(e: PointerEvent) {
    if (!drag) return
    const { from, tops, heights } = drag
    const last = tops.length - 1
    const card = cards[orderedModules[from].id]
    // Kept within the list.
    const dy = Math.min(
      Math.max(pointerY(e, card) - drag.startY, tops[0] - tops[from]),
      tops[last] + heights[last] - (tops[from] + heights[from]),
    )
    // A card passes another once its leading edge crosses the other's middle:
    // its bottom going down, its top going up. With the middles, the last
    // place could never be reached (the card stops at the end of the list).
    const top = tops[from] + dy
    const bottom = top + heights[from]
    drag.dy = dy
    drag.to = tops.filter((t, i) => {
      const middle = t + heights[i] / 2
      return i < from ? middle < top : i > from && middle < bottom
    }).length
  }

  // Where each card is drawn while dragging, relative to its place.
  function shift(index: number): number {
    if (!drag) return 0
    const { from, to } = drag
    if (index === from) return drag.dy
    if (from < index && index <= to) return -drag.span
    if (to <= index && index < from) return drag.span
    return 0
  }

  async function endReorder() {
    if (!drag) return
    const { from, to } = drag
    const order = orderedModules.map((m) => m.id)
    const [moved] = order.splice(from, 1)
    order.splice(to, 0, moved)
    // Same update: the list takes its new order as the shifts disappear, and
    // the grabbed card glides from where it was dropped to its place.
    drag = null
    if (from === to) return
    localOrder = order
    await run('set_module_order', { order })
    localOrder = null
  }

  // The screen shown as selected: the chosen one, or the primary one by default.
  const selectedScreen = $derived(
    settings?.screen ?? settings?.screens.find((s) => s.primary)?.id ?? '',
  )
  const chosenUnplugged = $derived(
    !!settings?.screen && !settings.screens.some((s) => s.id === settings?.screen),
  )

  const resistance = $derived<ResistanceChoice>(
    settings?.cursorResistance ? settings.resistanceStrength : 'off',
  )

  function load(next: Settings) {
    settings = next
    if (!drag) localOrder = null
  }

  async function run(command: string, args: Record<string, unknown> = {}) {
    error = null
    try {
      await invoke(command, args)
    } catch (e) {
      error = String(e)
    }
  }

  // Copies the report (version, system, end of the log) for the user to
  // send; nothing leaves the machine from here (ADR-0014).
  let copied = $state(false)
  let copiedTimer: ReturnType<typeof setTimeout> | undefined
  async function copyReport() {
    error = null
    try {
      await navigator.clipboard.writeText(await invoke<string>('crash_report'))
      copied = true
      clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => (copied = false), 2000)
    } catch {
      error = t('settings.log.copy_failed')
    }
  }

  // Position of the speed slider, follows the saved setting.
  let speedIndex = $state(1)
  $effect(() => {
    if (settings) {
      speedIndex = Math.max(
        0,
        speedChoices.findIndex((c) => c.value === settings?.notchSpeed),
      )
    }
  })

  // Position of the notched slider, follows the saved setting.
  let resistanceIndex = $state(0)
  $effect(() => {
    resistanceIndex = Math.max(
      0,
      resistanceChoices.findIndex((c) => c.value === resistance),
    )
  })

  function setResistance(choice: ResistanceChoice) {
    void run('set_cursor_resistance', {
      enabled: choice !== 'off',
      strength: choice === 'off' ? (settings?.resistanceStrength ?? 'medium') : choice,
    })
  }

  // What the settings say about updates.
  function updateText(update: UpdateStatus): string {
    switch (update.state) {
      case 'checking':
        return t('settings.updates.checking')
      case 'upToDate':
        return t('settings.updates.up_to_date')
      case 'available':
        return t('settings.updates.available', { version: update.version })
      case 'downloading':
        return update.percent === null
          ? t('settings.updates.downloading')
          : t('settings.updates.downloading_percent', { percent: update.percent })
      default:
        return t('settings.updates.idle')
    }
  }

  $effect(() => {
    // Follows the download without rebuilding the whole settings.
    const unlistenUpdate = listen<UpdateStatus>('update-changed', (e) => {
      if (settings) settings.update = e.payload
    })
    const unlisten = listen<Settings>('settings-changed', (e) => load(e.payload))
    const refresh = () => {
      // A companion pack may have been dropped in its folder (DF-0025).
      void invoke('reload_companions')
      return invoke<Settings>('get_settings').then(load)
    }
    void refresh()
    // A monitor may have been plugged in or out while the window was hidden.
    window.addEventListener('focus', refresh)
    getVersion().then((v) => (version = v))
    return () => {
      window.removeEventListener('focus', refresh)
      void unlisten.then((off) => off())
      void unlistenUpdate.then((off) => off())
    }
  })
</script>

<div class="layout">
  <nav aria-label={t('settings.categories')}>
    <div class="brand">
      <Logo />
      <span class="name">
        Minim Notch
        {#if version}<span class="version">v{version}</span>{/if}
      </span>
    </div>
    {#each pages as item (item.id)}
      <button
        class="nav-item"
        class:bottom={item.id === 'about'}
        class:current={page === item.id}
        aria-current={page === item.id ? 'page' : undefined}
        onclick={() => show(item.id)}
      >
        {t(item.key)}
      </button>
    {/each}
  </nav>

  <main>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if settings?.crash}
      <section class="crash" role="alert">
        <div class="label">{t('settings.crash.title')}</div>
        <div class="hint">
          {t('settings.crash.hint', {
            date: new Date(settings.crash.at).toLocaleString(settings.shownLanguage),
          })}
        </div>
        <div class="actions">
          <button class="primary" onclick={copyReport}>
            {copied ? t('settings.log.copied') : t('settings.log.copy_report')}
          </button>
          <button onclick={() => run('open_log')}>{t('settings.log.open')}</button>
          <button onclick={() => run('dismiss_crash')}>{t('settings.crash.dismiss')}</button>
        </div>
      </section>
    {/if}

    {#if settings && (settings.update.state === 'available' || settings.update.state === 'downloading')}
      <section class="update" role="status">
        <div class="label">
          {t('settings.updates.available', { version: settings.update.version })}
        </div>
        {#if settings.update.state === 'available' && settings.update.notes}
          <div class="hint notes">{settings.update.notes}</div>
        {/if}
        <div class="hint">{t('settings.updates.install.hint')}</div>
        <div class="actions">
          <button
            class="primary"
            disabled={settings.update.state === 'downloading'}
            onclick={() => run('install_update')}
          >
            {settings.update.state === 'downloading'
              ? updateText(settings.update)
              : t('settings.updates.install')}
          </button>
        </div>
      </section>
    {/if}

    {#if settings}
      {#if page === 'general'}
        <h1>{t('settings.page.general')}</h1>
        <section>
          <label class="row">
            <div>
              <div class="label">{t('settings.language')}</div>
              <div class="hint">{t('settings.language.hint')}</div>
            </div>
            <!-- Each language under its own name, readable whatever is shown. -->
            <select
              value={settings.language ?? ''}
              onchange={(e) => run('set_language', { language: e.currentTarget.value || null })}
            >
              <option value="">{t('settings.appearance.system')}</option>
              {#each settings.languages as [code, name] (code)}
                <option value={code} lang={code}>{name}</option>
              {/each}
            </select>
          </label>
          <label class="row separated">
            <div class="label">{t('settings.autostart')}</div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.autostart}
              onchange={(e) => run('set_autostart', { enabled: e.currentTarget.checked })}
            />
          </label>
          <label class="row">
            <div>
              <div class="label">{t('settings.sound')}</div>
              <div class="hint">{t('settings.sound.hint')}</div>
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.soundEnabled}
              onchange={(e) => run('set_sound', { enabled: e.currentTarget.checked })}
            />
          </label>
        </section>
      {:else if page === 'display'}
        <h1>{t('settings.page.display')}</h1>
        <section>
          <h2>{t('settings.group.look')}</h2>
          <div class="stack">
            <div>
              <div class="label">{t('settings.appearance')}</div>
              <div class="hint">{t('settings.appearance.hint')}</div>
            </div>
            <div
              class="segmented"
              style:grid-template-columns="repeat(3, 1fr)"
              role="radiogroup"
              aria-label={t('settings.appearance')}
            >
              {#each appearanceChoices as choice (choice.value)}
                <button
                  role="radio"
                  aria-checked={settings.appearance === choice.value}
                  class:selected={settings.appearance === choice.value}
                  onclick={() => run('set_appearance', { appearance: choice.value })}
                >
                  {t(choice.key)}
                </button>
              {/each}
            </div>
          </div>
          <div class="stack separated">
            <div>
              <div class="label">{t('settings.color')}</div>
            </div>
            <div class="swatches" role="radiogroup" aria-label={t('settings.color')}>
              <button
                class="swatch default"
                role="radio"
                aria-checked={settings.notchColor === null}
                class:selected={settings.notchColor === null}
                onclick={() => run('set_notch_color', { color: null })}
              >
                {t('settings.default')}
              </button>
              {#each notchColors as color (color)}
                <button
                  class="swatch"
                  role="radio"
                  aria-checked={settings.notchColor === color}
                  aria-label={color}
                  class:selected={settings.notchColor === color}
                  style:background={color}
                  onclick={() => run('set_notch_color', { color })}
                ></button>
              {/each}
              <!-- Any color: the system's picker. -->
              <label
                class="swatch picker"
                class:selected={settings.notchColor !== null && !notchColors.includes(settings.notchColor)}
                style:background={settings.notchColor !== null && !notchColors.includes(settings.notchColor)
                  ? settings.notchColor
                  : null}
                title={t('settings.color.custom')}
              >
                <input
                  type="color"
                  value={settings.notchColor ?? '#000000'}
                  onchange={(e) => run('set_notch_color', { color: e.currentTarget.value })}
                />
                <Palette size={14} />
              </label>
            </div>
          </div>
          <label class="row separated">
            <div>
              <div class="label">{t('settings.opacity')}</div>
              <div class="hint">{t('settings.percent', { value: opacity })}</div>
            </div>
            <input
              type="range"
              min="0"
              max="100"
              step="5"
              bind:value={opacity}
              oninput={(e) => previewNotchLook({ opacity: e.currentTarget.valueAsNumber, grain })}
              onchange={() => run('set_notch_opacity', { opacity })}
            />
          </label>
          <label class="row">
            <div>
              <div class="label">{t('settings.grain')}</div>
              <div class="hint">{t('settings.percent', { value: grain })}</div>
            </div>
            <input
              type="range"
              min="0"
              max={MAX_GRAIN}
              step="5"
              bind:value={grain}
              oninput={(e) => previewNotchLook({ opacity, grain: e.currentTarget.valueAsNumber })}
              onchange={() => run('set_notch_grain', { grain })}
            />
          </label>
          <div class="stack separated">
            <div>
              <div class="label">{t('settings.style')}</div>
              <div class="hint">{t('settings.style.hint')}</div>
            </div>
            <div
              class="segmented"
              style:grid-template-columns="repeat(2, 1fr)"
              role="radiogroup"
              aria-label={t('settings.style')}
            >
              {#each styleChoices as choice (choice.value)}
                <button
                  role="radio"
                  aria-checked={settings.style === choice.value}
                  class:selected={settings.style === choice.value}
                  onclick={() => run('set_style', { style: choice.value })}
                >
                  {t(choice.key)}
                </button>
              {/each}
            </div>
          </div>
          {#if settings.style === 'pill'}
            <label class="row separated">
              <div>
                <div class="label">{t('settings.gap')}</div>
                <div class="hint">{t('settings.pixels', { value: gap })}</div>
              </div>
              <input
                type="range"
                min="4"
                max="40"
                step="1"
                bind:value={gap}
                onchange={() => run('set_gap', { gap })}
              />
            </label>
          {:else}
            <!-- Attached: pins can curve outwards like the notch (DF-0028). -->
            <label class="row separated">
              <div>
                <div class="label">{t('settings.pin_flares')}</div>
                <div class="hint">{t('settings.pin_flares.hint')}</div>
              </div>
              <input
                type="checkbox"
                class="switch"
                checked={settings.pinFlares}
                onchange={(e) => run('set_pin_flares', { enabled: e.currentTarget.checked })}
              />
            </label>
          {/if}
        </section>
        <section>
          <h2>{t('settings.group.position')}</h2>
          <div class="stack">
            <div>
              <div class="label">{t('settings.edge')}</div>
              <div class="hint">{t('settings.edge.hint')}</div>
            </div>
            <div class="segmented" role="radiogroup" aria-label={t('settings.edge')}>
              {#each edgeChoices as choice (choice.value)}
                <button
                  role="radio"
                  aria-checked={settings.edge === choice.value}
                  class:selected={settings.edge === choice.value}
                  onclick={() => run('set_edge', { edge: choice.value })}
                >
                  {t(choice.key)}
                </button>
              {/each}
            </div>
          </div>
          <label class="row separated">
            <div>
              <div class="label">{t('settings.move')}</div>
              <div class="hint">{t('settings.move.hint')}</div>
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.movable}
              onchange={(e) => run('set_movable', { movable: e.currentTarget.checked })}
            />
          </label>
          {#if settings.offCenter}
            <div class="row">
              <div class="hint">{t('settings.off_center')}</div>
              <button onclick={() => run('recenter')}>{t('settings.recenter')}</button>
            </div>
          {/if}
          {#if settings.screens.length > 1 || chosenUnplugged}
            <label class="row separated">
              <div>
                <div class="label">{t('settings.screen')}</div>
                <div class="hint">{t('settings.screen.hint')}</div>
              </div>
              <select
                value={selectedScreen}
                onchange={(e) => run('set_screen', { screen: e.currentTarget.value })}
              >
                {#each settings.screens as screen (screen.id)}
                  <option value={screen.id}>{screen.label}</option>
                {/each}
                {#if chosenUnplugged}
                  <!-- Only while the chosen screen is unplugged: says why the
                       notch is on the primary screen for now. -->
                  <option value={settings.screen}>{t('settings.screen.unplugged')}</option>
                {/if}
              </select>
            </label>
          {/if}
        </section>
        <section>
          <h2>{t('settings.group.behavior')}</h2>
          <label class="row">
            <div>
              <div class="label">{t('settings.auto_hide')}</div>
              <div class="hint">{t('settings.auto_hide.hint')}</div>
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.autoHide}
              onchange={(e) => run('set_auto_hide', { enabled: e.currentTarget.checked })}
            />
          </label>
          {#if settings.autoHide}
            <div class="row">
              <div>
                <div class="label">{t('settings.auto_hide.delay')}</div>
                <div class="hint">{t('settings.seconds', { value: seconds.format(hideDelay) })}</div>
              </div>
              <!-- A mark at the default; the slider stops there when it
                   passes close by. -->
              <div class="marked">
                <input
                  type="range"
                  min="0"
                  max={HIDE_DELAY_MAX}
                  step="0.1"
                  aria-label={t('settings.auto_hide.delay')}
                  bind:value={hideDelay}
                  oninput={() => {
                    if (Math.abs(hideDelay - HIDE_DELAY_DEFAULT) <= 0.25) {
                      hideDelay = HIDE_DELAY_DEFAULT
                    }
                  }}
                  onchange={saveHideDelay}
                />
                <div class="stops">
                  <button
                    class:selected={hideDelay === HIDE_DELAY_DEFAULT}
                    style:left="{(HIDE_DELAY_DEFAULT / HIDE_DELAY_MAX) * 100}%"
                    onclick={() => {
                      hideDelay = HIDE_DELAY_DEFAULT
                      saveHideDelay()
                    }}
                  >
                    {t('settings.default')}
                  </button>
                </div>
              </div>
            </div>
          {/if}
          <div class="stack separated">
            <div>
              <div class="label">{t('settings.speed')}</div>
              <div class="hint">{t('settings.speed.hint')}</div>
            </div>
            <!-- Notched slider, like the cursor resistance one. -->
            <div class="notched">
              <input
                type="range"
                min="0"
                max={speedChoices.length - 1}
                step="1"
                aria-label={t('settings.speed')}
                aria-valuetext={t(speedChoices[speedIndex].key)}
                bind:value={speedIndex}
                onchange={() => run('set_notch_speed', { speed: speedChoices[speedIndex].value })}
              />
              <div class="stops">
                {#each speedChoices as choice, i (choice.value)}
                  <button
                    class:selected={speedIndex === i}
                    style:left="{(i / (speedChoices.length - 1)) * 100}%"
                    onclick={() => {
                      speedIndex = i
                      void run('set_notch_speed', { speed: choice.value })
                    }}
                  >
                    {t(choice.key)}
                  </button>
                {/each}
              </div>
            </div>
          </div>
          <label class="row separated">
            <div>
              <div class="label">{t('settings.fullscreen')}</div>
              <div class="hint">
                {t('settings.fullscreen.hint')}
                {#if settings.resistanceAvailable}
                  {t('settings.fullscreen.resistance')}
                {/if}
              </div>
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.hideInFullscreen}
              onchange={(e) => run('set_hide_in_fullscreen', { enabled: e.currentTarget.checked })}
            />
          </label>
        </section>
        <section>
          <h2>{t('settings.group.modules')}</h2>
          <div class="stack">
            <div>
              <div class="label">{t('settings.layout')}</div>
              <div class="hint">{t('settings.layout.hint')}</div>
            </div>
            <div
              class="segmented"
              style:grid-template-columns="repeat(2, 1fr)"
              role="radiogroup"
              aria-label={t('settings.layout')}
            >
              {#each layoutChoices as choice (choice.value)}
                <button
                  role="radio"
                  aria-checked={settings.moduleLayout === choice.value}
                  class:selected={settings.moduleLayout === choice.value}
                  onclick={() => run('set_module_layout', { layout: choice.value })}
                >
                  {t(choice.key)}
                </button>
              {/each}
            </div>
          </div>
        </section>
        <section>
          <h2>{t('settings.cursor')}</h2>
          <div class="stack">
            <div>
              <div class="label">{t('settings.resistance')}</div>
              <div class="hint">
                {#if !settings.resistanceAvailable}
                  {t('settings.resistance.windows_only')}
                {:else if resistance === 'impassable'}
                  {t('settings.resistance.impassable_hint')}
                {:else}
                  {t('settings.resistance.hint')}
                {/if}
              </div>
            </div>
            <!-- Notched slider: one stop per preset, labels under the stops. -->
            <div class="notched">
              <input
                type="range"
                min="0"
                max={resistanceChoices.length - 1}
                step="1"
                aria-label={t('settings.resistance')}
                aria-valuetext={t(resistanceChoices[resistanceIndex].key)}
                disabled={!settings.resistanceAvailable}
                bind:value={resistanceIndex}
                onchange={() => setResistance(resistanceChoices[resistanceIndex].value)}
              />
              <div class="stops">
                {#each resistanceChoices as choice, i (choice.value)}
                  <button
                    class:selected={resistanceIndex === i}
                    style:left="{(i / (resistanceChoices.length - 1)) * 100}%"
                    disabled={!settings.resistanceAvailable}
                    onclick={() => {
                      resistanceIndex = i
                      setResistance(choice.value)
                    }}
                  >
                    {t(choice.key)}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </section>
      {:else if page === 'companions'}
        <h1>{t('settings.page.companions')}</h1>
        <CustomCompanions />
      {:else if page === 'modules' && openModule}
        {@const ModuleSettings = moduleUis[openModule.id]?.settings}
        <button class="back" onclick={() => (openModuleId = null)}>← {t('settings.page.modules')}</button>
        <h1>{openModule.name}</h1>
        <section>
          <label class="row">
            <div>
              <div class="label">{t('settings.module.enabled')}</div>
              {#if openModule.description}<div class="hint">{openModule.description}</div>{/if}
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={openModule.enabled}
              onchange={(e) => setModuleEnabled(openModule, e.currentTarget.checked)}
            />
          </label>
        </section>
        {#if openModule.enabled && ModuleSettings}
          <section>
            <ModuleSettings
              data={openModule.settings}
              call={(action, args) => run('module_call', { id: openModule.id, action, args })}
            />
          </section>
        {/if}
      {:else if page === 'modules'}
        <h1>{t('settings.page.modules')}</h1>
        <p class="intro">{t('settings.modules.intro')}</p>
        {#each orderedModules as module, index (module.id)}
          {@const ui = moduleUis[module.id]}
          {@const Icon = ui?.icon ?? Puzzle}
          {@const configurable = module.enabled && !!ui?.settings}
          <section
            class="module-card"
            class:disabled={!module.enabled}
            class:dragged={drag?.from === index}
            class:sliding={drag !== null && drag.from !== index}
            style:transform={drag ? `translateY(${shift(index)}px)` : undefined}
            bind:this={cards[module.id]}
            animate:flip={{ duration: 150 }}
          >
            <button
              class="grip"
              title={t('settings.module.grip')}
              aria-label={t('settings.module.grip_label', { name: module.name })}
              onpointerdown={(e) => startReorder(e, module.id)}
              onpointermove={reorder}
              onpointerup={endReorder}
              onpointercancel={endReorder}
            >
              <GripVertical size={16} />
            </button>
            <div class="module-icon"><Icon size={20} /></div>
            <div class="module-text">
              <div class="label">{module.name}</div>
              {#if module.description}<div class="hint">{module.description}</div>{/if}
            </div>
            <div class="actions">
              {#if module.pin && module.pinnable}
                <select
                  aria-label={t('settings.module.pin_side_label', { name: module.name })}
                  title={settings.edge === 'left' || settings.edge === 'right'
                    ? t('settings.module.pin_side_vertical')
                    : t('settings.module.pin_side')}
                  value={module.pin}
                  onchange={(e) => setModulePin(module, e.currentTarget.value as PinSide)}
                >
                  {#each pinSideChoices as choice (choice.value)}
                    <option value={choice.value}>{t(choice.key)}</option>
                  {/each}
                </select>
              {/if}
              <button
                class="icon pin"
                class:pinned={!!module.pin && module.pinnable}
                disabled={!module.pinnable}
                aria-pressed={!!module.pin && module.pinnable}
                title={!module.enabled
                  ? t('settings.module.pin_enable_first')
                  : !module.pinnable
                    ? t('settings.module.pin_first')
                    : module.pin
                      ? t('settings.module.unpin')
                      : t('settings.module.pin')}
                aria-label={t('settings.module.pin_label', { name: module.name })}
                onclick={() => setModulePin(module, module.pin ? null : 'auto')}
              >
                <PinIcon size={16} />
              </button>
              <input
                type="checkbox"
                class="switch"
                aria-label={t('settings.module.enable_label', { name: module.name })}
                checked={module.enabled}
                onchange={(e) => setModuleEnabled(module, e.currentTarget.checked)}
              />
              <button
                class="icon"
                disabled={!configurable}
                title={configurable
                  ? t('settings.module.settings', { name: module.name })
                  : module.enabled
                    ? t('settings.module.no_settings')
                    : t('settings.module.enable_first')}
                aria-label={t('settings.module.settings', { name: module.name })}
                onclick={() => (openModuleId = module.id)}
              >
                <SettingsIcon size={16} />
              </button>
            </div>
          </section>
        {/each}
      {:else}
        <h1>{t('settings.page.about')}</h1>
        <section>
          <div class="row">
            <div class="label">{t('settings.version')}</div>
            <div>{version}</div>
          </div>
          <div class="stack separated">
            <div>
              <div class="label">{t('settings.updates')}</div>
              <div class="hint">
                {settings.updatesEnabled
                  ? updateText(settings.update)
                  : t('settings.updates.dev')}
              </div>
            </div>
            <div class="actions">
              <button
                disabled={!settings.updatesEnabled ||
                  settings.update.state === 'checking' ||
                  settings.update.state === 'downloading'}
                onclick={() => run('check_updates')}
              >
                {t('settings.updates.check')}
              </button>
            </div>
          </div>
          <label class="row">
            <div>
              <div class="label">{t('settings.updates.auto')}</div>
              <div class="hint">{t('settings.updates.auto.hint')}</div>
            </div>
            <input
              type="checkbox"
              class="switch"
              checked={settings.checkUpdates}
              onchange={(e) => run('set_check_updates', { enabled: e.currentTarget.checked })}
            />
          </label>
        </section>
        <section>
          <div class="row">
            <div>
              <div class="label">{t('settings.config_dir')}</div>
              <div class="hint path">{settings.configDir}</div>
            </div>
          </div>
          <div class="stack separated">
            <div>
              <div class="label">{t('settings.log')}</div>
              <div class="hint path">{settings.logFile}</div>
            </div>
            <div class="actions">
              <button onclick={() => run('open_log')}>{t('settings.log.open')}</button>
              <button onclick={() => run('reveal_log')}>{t('settings.log.reveal')}</button>
              <button onclick={copyReport}>
                {copied ? t('settings.log.copied') : t('settings.log.copy_report')}
              </button>
            </div>
          </div>
        </section>
        <section>
          <div class="stack">
            <div>
              <div class="label">{t('settings.help')}</div>
              <div class="hint">{t('settings.help.hint')}</div>
            </div>
            <div class="actions">
              <button onclick={() => run('open_link', { link: 'website' })}>
                {t('settings.help.website')}
              </button>
              <button onclick={() => run('open_link', { link: 'issues' })}>
                {t('settings.help.issues')}
              </button>
            </div>
          </div>
          <div class="row">
            <div>
              <div class="label">{t('settings.license')}</div>
              <div class="hint">{t('settings.license.hint')}</div>
            </div>
          </div>
        </section>
      {/if}
    {/if}
  </main>
</div>
