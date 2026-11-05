# DESIGN.md: Cooked

Live usage monitor (5h, week, tokens per minute) as a glass island under the macOS menu bar.
These are the rules every screen of the app follows. Read them before you change how something looks.

## What it should feel like

A measuring instrument, not a dashboard. It sits above your work all day: it must be quiet while everything is fine
and unmistakable when it is not. The reference is a car dashboard at night: everything dark and gray, a single red
warning light only when needed.

## Fixed rules

1. **Material.** Island and popover in Liquid Glass (`glassEffect(.regular)`). Inside, opaque cards and wells: never
   glass on glass. With "Reduce Transparency" the surfaces become opaque. No glow outside the lit dots. Default style:
   Darker, regular glass with an 80% `#161616` veil. System and Clear get the same veil, as light as passes WCAG over a
   white window (80% and 85%); red text smaller than the hero sits in a well, not on glass.
2. **Color.** Text and charts are white at three levels: 100% (data), 72% (secondary text), 55% (labels, axes). 55% is
   the minimum for text. Plus the **red** `#FF453A`, which appears only in urgency.
3. **Accounts without colors.** They are told apart by short name (`work`, `personal`, `side`, `codex`) and by fixed
   position in the list. The account that is consuming is at weight 500 and full color; the others at 72%.
4. **One type family.** Numbers always tabular. There is only one big number per view.
5. **Urgency:** the island keeps its capsule shape; the 5h % (in a well) and the hairline turn red, and the wave
   widens. The time you run out and the thread to check are in the popover. Outside urgency the red never appears, not
   even as an icon, with one exception: in the popover's histogram every minute that went over the threshold is red
   above the threshold row.
6. **Motion.** Transitions only for state changes (expanding, closing). The chart moves at 60 fps with continuous
   values, never in steps. No pulsing.

## Components

| Component | What it shows | Rule |
|---|---|---|
| Calm island | active account, 5h %, pace | one row, 240 px at most |
| Urgent island | the calm island, 5h % and hairline red | same size as calm; details only in the popover |
| Histogram (popover) | tokens/min for the last 60 min, the alert threshold (p99) | monochrome columns; red only above the threshold; small idle drops keep it moving, never into the threshold row |
| Notice card (popover) | what is missing, one sentence, and its one fix | one at a time, at the top; title in emphasis, body in ink2, both wrap; one primary button at most; the missing number shows "—" |
| Account list | 5h, full at, reset and week % per account; "blocked until" when the limit is hit | text and a thin bar, no colored dots; accounts not used in 5 h fold into one line |

## Don't

- Test a look on a gradient background: test it on top of a real working window.
- One color per account, multicolor stacked bars, gradient text.
- Glow, colored shadows, pulsing, green "live" dots.
- `ui-rounded` mixed with the system text; spaced small caps everywhere.

## Graphite and the dot matrix

The look takes the pill and the 3D settings of the hex app as a reference
(https://x.com/thdxr/status/2115621363889147972).

- **Signature object: the dot matrix.** Charts are made of LED dots on a fixed grid. Each dot has three levels: off
  (7%), halo (16%) and lit (warm white `#F4F1EA`). In the island the equalizer is a wave mirrored around the center
  row: when you are calm it is a line with a slow wave, during a peak it widens and turns red.
- **Exception to the no-glow rule.** Only the lit dots have a halo, because it is light emitted by the display and not
  a decoration. No other element has glow.
- **Motion.** The wave drifts slowly and the current minute grows: it is the data that moves, not a decoration. A
  single 60 fps cycle, continuous dot brightness; at the end of the minute each column moves to its neighbor's value
  on a 0.5 s spring, and the dots stay on the grid.
- **Sober matrix:** dots only in the charts (equalizer, histogram, account bars). Numbers and text are text.
- **Surfaces sit on top, data sits inside.** The island and the cards are raised (`#161616`, two soft shadows, 0.5 px
  hairline at 16%, top hairline at 6%). The wave, the histogram and the account bars sit in recessed wells (gradient
  `#080808` → `#1b1b1b`, inner shadow at the top, hairline at 7%). The popover is a `#080808` page with two cards. In
  urgency the island's hairline turns red.

## Typography

IBM Plex Sans for text, IBM Plex Mono for every number. The woff2 files are included (SIL OFL license).

| Role | Typeface | Size | Weight |
|---|---|---|---|
| Label, axes, notes | Plex Sans | 12 px, color 55% | 400 |
| Text | Plex Sans | 13 px, line height 18 | 400 |
| Highlighted data, active account, button | Plex Sans or Mono | 13 px | 500 |
| Big number (one per view) | Plex Mono | 32 px, tracking −0.03em | 500 |
| Numbers, times, percentages | Plex Mono | same as the surrounding text | 400 |

Only two weights: 400 and 500. No 600 and no 700. The hierarchy comes from color (100/72/55%) and position.
Every gray text has a contrast of at least 4.5:1 on the glass (55% = 5.7:1, 72% = 9.2:1, WCAG calculation on
`#0B0B0C`); 36% gray was too hard to read (3:1).

## Motion and shape

Emil Kowalski's rules, with hex's springs.

- Springs with no bounce: controls 0.2 s, popover 0.24 s, island changing shape 0.3 s.
- Press: button `scale(.97)`, island `scale(.98)`. It goes down in 80 ms and returns on the spring in 0.18 s.
- Popover: enters from `scale(.96)` and opacity 0, origin top center, 0.24 s spring. It leaves faster (200 ms
  ease-out). It closes with Esc or a click outside.
- Calm → urgent: no change of shape, only color.
- A number that changes scrolls: the old one goes up, the new one comes in from below, with a 2 px blur.
- Concentric radii: inner radius = outer radius − padding (popover 24, card 16, well 8).
- "Reduce Motion": no transitions, wave and histogram still.
- The button has a focus outline visible from the keyboard.
