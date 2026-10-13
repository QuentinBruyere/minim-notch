<script lang="ts">
  import { t, type Key } from '../../lib/i18n.svelte'
  import type { ModuleSettingsProps } from '..'

  // Mirrors `Settings` in src-tauri/src/modules/timer/mod.rs
  interface Data {
    defaultMinutes: number
    favorites: number[]
    stepSeconds: number
  }

  // The less / more steps offered (`STEPS` in Rust), in seconds.
  const steps = [30, 60, 120, 300]
  const stepText = (seconds: number) =>
    seconds % 60 === 0
      ? t('timer.minutes', { minutes: seconds / 60 })
      : t('timer.seconds', { seconds })

  let { data: raw, call }: ModuleSettingsProps = $props()
  const data = $derived(raw as Data)

  // Text fields are edited locally and applied on demand.
  let duration = $state('')
  let favorites = $state('')
  $effect(() => {
    duration = String(data.defaultMinutes)
    favorites = data.favorites.join(', ')
  })

  // "5, 15, 25" -> [5, 15, 25]; anything else is left for the module to refuse.
  const parsedFavorites = $derived(
    favorites
      .split(/[\s,;]+/)
      .filter(Boolean)
      .map(Number),
  )
</script>

<form
  class="row separated"
  onsubmit={(e) => {
    e.preventDefault()
    void call('update', { ...data, defaultMinutes: Number(duration) })
  }}
>
  <div>
    <div class="label">{t('timer.default')}</div>
    <div class="hint">{t('timer.default_hint')}</div>
  </div>
  <div class="field">
    <input type="number" min="0" max="600" bind:value={duration} />
    <button disabled={duration === String(data.defaultMinutes)}>{t('settings.apply')}</button>
  </div>
</form>
<form
  class="row separated"
  onsubmit={(e) => {
    e.preventDefault()
    void call('update', { ...data, favorites: parsedFavorites })
  }}
>
  <div>
    <div class="label">{t('timer.favorites_setting')}</div>
    <div class="hint">{t('timer.favorites_hint')}</div>
  </div>
  <div class="field">
    <input type="text" placeholder="5, 15, 25" bind:value={favorites} />
    <button disabled={favorites === data.favorites.join(', ')}>{t('settings.apply')}</button>
  </div>
</form>
<label class="row separated">
  <div>
    <div class="label">{t('timer.step')}</div>
    <div class="hint">{t('timer.step_hint')}</div>
  </div>
  <select
    value={data.stepSeconds}
    onchange={(e) => void call('update', { ...data, stepSeconds: Number(e.currentTarget.value) })}
  >
    {#each steps as step (step)}
      <option value={step}>{stepText(step)}</option>
    {/each}
  </select>
</label>
