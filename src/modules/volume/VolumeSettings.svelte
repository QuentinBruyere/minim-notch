<script lang="ts">
  import { t } from '../../lib/i18n.svelte'
  import type { ModuleSettingsProps } from '..'

  // Mirrors `Settings` in src-tauri/src/modules/volume/mod.rs
  interface Data {
    replaceWindows: boolean
  }

  let { data: raw, call }: ModuleSettingsProps = $props()
  const data = $derived(raw as Data)

  // The module takes its whole settings object at once.
  function update(change: Partial<Data>) {
    void call('update', { ...data, ...change })
  }
</script>

<!-- The volume keys handled here, Windows' window gone (ADR-0020). -->
<label class="row separated">
  <div>
    <div class="label">{t('volume.replace_windows')}</div>
    <div class="hint">{t('volume.replace_windows_hint')}</div>
  </div>
  <input
    type="checkbox"
    class="switch"
    checked={data.replaceWindows}
    onchange={(e) => update({ replaceWindows: e.currentTarget.checked })}
  />
</label>
