import type { LucideProps } from '@lucide/svelte'
import Minus from '@lucide/svelte/icons/minus'
import Pause from '@lucide/svelte/icons/pause'
import Play from '@lucide/svelte/icons/play'
import Plus from '@lucide/svelte/icons/plus'
import RotateCcw from '@lucide/svelte/icons/rotate-ccw'
import SkipBack from '@lucide/svelte/icons/skip-back'
import SkipForward from '@lucide/svelte/icons/skip-forward'
import Volume from '@lucide/svelte/icons/volume'
import Volume1 from '@lucide/svelte/icons/volume-1'
import Volume2 from '@lucide/svelte/icons/volume-2'
import VolumeX from '@lucide/svelte/icons/volume-x'
import type { Component } from 'svelte'
import type { Icon } from './content'

// The Lucide icon of each `Icon` a module can name (buttons, `Item::icon`).
export const icons: Record<Icon, Component<LucideProps>> = {
  play: Play,
  pause: Pause,
  reset: RotateCcw,
  volume_low: Volume,
  volume_medium: Volume1,
  volume_high: Volume2,
  muted: VolumeX,
  previous: SkipBack,
  next: SkipForward,
  minus: Minus,
  plus: Plus,
}

// The icons drawn filled on buttons (closed shapes: play, pause, skip); the
// others stay outlined.
export const filled: ReadonlySet<Icon> = new Set<Icon>(['play', 'pause', 'previous', 'next'])
