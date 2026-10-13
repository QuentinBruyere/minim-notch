<script lang="ts">
  import { untrack } from 'svelte'

  // A compact view's text (closed notch, pin). Too long for its place: cut
  // with an ellipsis, or, if `scroll`, it slides left until its end reaches
  // the right edge, stays a moment, fades out and comes back at its start,
  // like a car radio (DF-0017); once per text, then cut like the others
  // (DF-0033). The view showing it says when it has already scrolled
  // (`scroll` false): it outlives this component, redrawn when the notch
  // opens and closes.
  let {
    text,
    scroll,
    dimmed = false,
    onturn,
    onscrolled,
  }: {
    text: string
    scroll: boolean
    dimmed?: boolean
    // Told, as a slide starts, how long until the text has been read once:
    // its end at the right edge, a moment there, then faded out; 0 if it
    // fits. A spotlight ends there, before the text comes back at its start.
    onturn?: (ms: number) => void
    // Told once the text has scrolled to its end and come back.
    onscrolled?: () => void
  } = $props()

  const PX_PER_SECOND = 30
  // Still at the start, still at the end, fading out, fading back in.
  const START_MS = 2000
  const END_MS = 1500
  const FADE_MS = 300

  let box = $state<HTMLElement>()
  let inner = $state<HTMLElement>()
  let boxWidth = $state(0)
  // The whole text's width; the box may be narrower.
  let textWidth = $state(0)
  $effect(() => {
    void text
    void boxWidth
    if (box) textWidth = box.scrollWidth
  })
  // How far the text has to go for its end to reach the right edge.
  const overflow = $derived(Math.max(0, textWidth - boxWidth))
  // Scrolled once: a new text scrolls again.
  let done = $state(false)
  $effect(() => {
    void text
    done = false
  })
  const moving = $derived(scroll && overflow > 1 && !done)

  // Times in ms, turned into the offsets of one turn.
  $effect(() => {
    // Not a dependency: the spotlight ending must not restart the slide.
    const report = (ms: number) => untrack(() => onturn?.(ms))
    if (!moving || !inner) {
      report(0)
      return
    }
    const slide = (overflow / PX_PER_SECOND) * 1000
    report(START_MS + slide + END_MS + FADE_MS)
    // Then it fades out and comes back at its start, still.
    const marks = [START_MS, slide, END_MS, FADE_MS, FADE_MS]
    const total = marks.reduce((a, b) => a + b, 0)
    let at = 0
    const offset = (ms: number) => (at += ms) / total
    const end = `translateX(${-overflow}px)`
    const animation = inner.animate(
      [
        { offset: 0, transform: 'translateX(0)', opacity: 1 },
        { offset: offset(START_MS), transform: 'translateX(0)', opacity: 1 },
        { offset: offset(slide), transform: end, opacity: 1 },
        { offset: offset(END_MS), transform: end, opacity: 1 },
        { offset: offset(FADE_MS), transform: end, opacity: 0 },
        { offset: at / total, transform: 'translateX(0)', opacity: 0 },
        { offset: offset(FADE_MS), transform: 'translateX(0)', opacity: 1 },
      ],
      { duration: total, easing: 'linear' },
    )
    animation.onfinish = () => {
      done = true
      untrack(() => onscrolled?.())
    }
    return () => animation.cancel()
  })
</script>

<span class="label marquee" class:moving class:dimmed bind:this={box} bind:clientWidth={boxWidth}>
  <span class="text" bind:this={inner}>{text}</span>
</span>

<style>
  .marquee.moving {
    text-overflow: clip;
  }

  .moving .text {
    display: inline-block;
  }
</style>
