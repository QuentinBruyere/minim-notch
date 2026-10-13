import type { LucideProps } from '@lucide/svelte'
import SquareTerminal from '@lucide/svelte/icons/square-terminal'
import Clock from '@lucide/svelte/icons/clock'
import SquareDashed from '@lucide/svelte/icons/square-dashed'
import PawPrint from '@lucide/svelte/icons/paw-print'
import Hourglass from '@lucide/svelte/icons/hourglass'
import Music from '@lucide/svelte/icons/music'
import TimerIcon from '@lucide/svelte/icons/timer'
import Volume2 from '@lucide/svelte/icons/volume-2'
import type { Component } from 'svelte'
import ClaudeCodeSettings from './claude-code/ClaudeCodeSettings.svelte'
import ClockSettings from './clock/ClockSettings.svelte'
import CompanionSettings from './companion/CompanionSettings.svelte'
import MediaSettings from './media/MediaSettings.svelte'
import TimerSettings from './timer/TimerSettings.svelte'
import VolumeSettings from './volume/VolumeSettings.svelte'

// What a module's settings component receives (ADR-0009).
export interface ModuleSettingsProps {
  // The module's own data, from `Module::settings` in Rust.
  data: unknown
  // Runs one of the module's actions (`Module::call`); errors are shown by the window.
  call: (action: string, args?: Record<string, unknown>) => Promise<void>
}

// Front-end side of a module: its icon (Lucide) and its settings page, if any.
export interface ModuleUi {
  icon: Component<LucideProps>
  settings?: Component<ModuleSettingsProps>
}

// By module id. A build that adds modules passes their entries to `start`.
export type ModuleUis = Record<string, ModuleUi>

// The modules shipped with the core.
export const coreModuleUis: ModuleUis = {
  blank: { icon: SquareDashed },
  'claude-code': { icon: SquareTerminal, settings: ClaudeCodeSettings },
  clock: { icon: Clock, settings: ClockSettings },
  companion: { icon: PawPrint, settings: CompanionSettings },
  media: { icon: Music, settings: MediaSettings },
  stopwatch: { icon: TimerIcon },
  timer: { icon: Hourglass, settings: TimerSettings },
  volume: { icon: Volume2, settings: VolumeSettings },
}
