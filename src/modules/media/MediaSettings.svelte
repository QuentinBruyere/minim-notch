<script lang="ts">
  import { t } from '../../lib/i18n.svelte'
  import type { ModuleSettingsProps } from '..'

  // Mirrors `Settings` in src-tauri/src/modules/media/mod.rs
  interface Data {
    scroll: boolean
    hideText: boolean
  }

  let { data: raw, call }: ModuleSettingsProps = $props()
  const data = $derived(raw as Data)

  // The module takes its whole settings object at once.
  function update(change: Partial<Data>) {
    void call('update', { ...data, ...change })
  }
</script>

<label class="row separated">
  <div>
    <div class="label">{t('media.scroll')}</div>
    <div class="hint">{t('media.scroll_hint')}</div>
  </div>
  <input
    type="checkbox"
    class="switch"
    checked={data.scroll}
    onchange={(e) => update({ scroll: e.currentTarget.checked })}
  />
</label>
<label class="row separated">
  <div>
    <div class="label">{t('media.hide_text')}</div>
    <div class="hint">{t('media.hide_text_hint')}</div>
  </div>
  <input
    type="checkbox"
    class="switch"
    checked={data.hideText}
    onchange={(e) => update({ hideText: e.currentTarget.checked })}
  />
</label>
