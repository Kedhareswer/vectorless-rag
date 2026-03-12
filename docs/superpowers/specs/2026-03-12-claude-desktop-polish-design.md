# TGG UI Polish: Claude Desktop DNA

**Date:** 2026-03-12
**Approach:** CSS-first (minimal JS for interactive behaviors like auto-grow fallback, scroll detection, platform detection)
**Goal:** Elevate TGG from "solid Raycast-inspired app" to "Claude Desktop-level polish" across 7 layers

---

## Context

TGG has a working Raycast-inspired layout: top bar, full-width chat, slide-over panels, dark/light modes, peach accent (#DE7356). The foundation is solid but lacks the hundreds of micro-details that make Claude Desktop feel effortless. This spec covers all 7 polish layers needed to close that gap using only CSS animations, transitions, and custom properties.

### What stays the same
- Layout architecture (TopBar + full-width chat + overlay panels)
- Color system (warm neutrals + peach accent)
- Component structure (React + CSS Modules + Zustand)
- All Tauri IPC and backend code

### What changes
- Typography tokens (larger, more generous)
- Animation system (choreographed CSS keyframes)
- Input bar (auto-grow textarea, refined focus states)
- Every interactive element gets hover/active/focus states
- Scrollbars, selection color, edge fading, window integration

---

## 1. Typography & Markdown Rendering

### Token changes

| Token | Current | New |
|-------|---------|-----|
| `--text-base` | 13px | 15px |
| `--text-sm` | 12px | 13px |
| `--text-xs` | 11px | 12px |
| `--text-lg` | 15px | 18px |
| `--text-xl` | 18px | 22px |

New tokens:
- `--line-height-body: 1.7` — replaces existing `--line-height-relaxed: 1.6`. Search-replace all references to `--line-height-relaxed` with `--line-height-body` across the codebase, then delete the old token.
- `--line-height-tight: 1.3`

### Assistant message prose styles

Added as nested selectors inside the existing `.assistantContent` class in `ChatPanel.module.css` (CSS Modules scoped, not global). The existing `.assistantContent` already has some markdown rules — these replace and extend them:

- **Paragraphs:** 15px, line-height 1.7, `--text-primary`, margin-bottom 1em
- **Headings (h1-h3):** bold, line-height 1.3, extra top margin (1.5em) for visual separation
- **Code blocks:** `var(--bg-surface)` background (distinct from content area in both themes), 1px `--border` border, `--radius-md` corners, JetBrains Mono 13px, 3px left border in `--accent`
- **Inline code:** `--bg-hover` background, 1px `--border-light` border, 2px horizontal padding
- **Blockquotes:** 3px left border `--accent`, italic, `--text-secondary` color, left padding 16px
- **Lists:** proper indentation, `::marker` color `--text-tertiary`
- **Tables:** `--border-light` grid, alternating row tint `--bg-hover` on even rows
- **Links:** `--accent` color, underline on hover only
- **Horizontal rules:** 1px `--border-light`, margin 1.5em 0

### User messages
Keep at current density (13px) since they're typically short.

### Files changed
- `src/styles/theme.css` — token updates, new tokens
- `src/components/chat/ChatPanel.module.css` — nested prose selectors inside existing `.assistantContent`

---

## 2. Thinking & Tool Use Animation

### Running state: 3-dot Claude pulse

Replace single pulsing dot with 3 dots:
- 3 circles, each 4px diameter, `--accent` color, spaced 4px apart
- `@keyframes claudePulse` — opacity 0.3 to 1.0 to 0.3, 1.4s ease-in-out infinite
- Staggered delays: dot 1 = 0ms, dot 2 = 150ms, dot 3 = 300ms
- Label fades in with `animation: fadeIn 200ms ease`

### Completion animation

When a step completes:
1. Dots collapse (scale to 0, 100ms)
2. Checkmark scales in: `@keyframes popIn` — scale(0) to scale(1.15) to scale(1), 250ms
3. Checkmark gets brief glow: `box-shadow: 0 0 8px var(--success)` fading over 400ms
4. Whole row does `@keyframes slideSettle` — translateX(-2px) to translateX(0), 150ms

### Step stagger-in

Each new step: `@keyframes stepAppear` — translateY(8px) opacity(0) to translateY(0) opacity(1), 200ms ease, with `animation-delay: n * 60ms`.

### Expand/collapse details

- **Architecture change:** The current code uses `{expanded && <div>...}` conditional rendering, which unmounts the DOM and prevents CSS animation. Change to always-rendered wrapper: the details `<div>` is always in the DOM, wrapped in a grid container. CSS controls visibility.
- Use `display: grid; grid-template-rows: 0fr` (collapsed) / `grid-template-rows: 1fr` (expanded) with `transition: grid-template-rows 250ms ease`. This avoids the broken easing of `max-height` when actual content is shorter than the max value.
- Inner content wrapper: `overflow: hidden; min-height: 0` (the grid child that actually collapses)
- Content opacity fades in with 100ms delay after height starts expanding (`transition: opacity 150ms ease 100ms`)
- The `expanded` state still controls classes: `.detailsGrid.expanded { grid-template-rows: 1fr }`, `.detailsGrid:not(.expanded) { grid-template-rows: 0fr }`

### Steps group container

- 2px left border `--border-light` connecting all steps vertically
- On all-steps-complete: add `.allComplete` class (computed in ThinkingBlock when `steps.every(s => s.status === 'complete')`), which triggers `@keyframes borderFlash` — `border-color` from `--border-light` to `--accent` and back. Use `animation: borderFlash 600ms ease 1; animation-fill-mode: none;` so the border returns to its default `--border-light` after the single flash. The class persists (all-complete doesn't revert) but the animation plays only once.

### Files changed
- `src/components/chat/ThinkingBlock.tsx` — 3-dot markup, completion state classes
- `src/components/chat/ThinkingBlock.module.css` — all new keyframes and animation classes

---

## 3. Input Bar Refinement

### Auto-growing textarea

The input already uses `<textarea>` with an existing JS auto-grow `useEffect` (ChatPanel.tsx ~line 100). Enhance:
- Add CSS `field-sizing: content` to the textarea styles — this enables native auto-grow in Chromium 123+ (Tauri's WebView2). When supported, it supersedes the JS fallback.
- Keep the existing JS `useEffect` auto-grow as fallback (it's harmless when `field-sizing` works, and essential when it doesn't).
- Min height: 44px, max height: 200px, `overflow-y: auto` beyond max

### Focus state progression

| State | Border | Shadow |
|-------|--------|--------|
| Resting | `1px solid var(--border)` | `var(--shadow-md)` |
| Hover | `1px solid var(--border)` | `var(--shadow-lg)` |
| Focus | `1px solid var(--accent)` | `0 0 0 3px var(--accent-soft), var(--shadow-lg)` |

All transitions: 150ms ease.

### Send button states

| State | Style |
|-------|-------|
| Disabled (empty) | `opacity: 0.3`, `color: var(--text-tertiary)`, `cursor: not-allowed` |
| Active (has text) | `opacity: 1`, `color: var(--accent)`, smooth transition |
| Hover (when active) | `background: var(--accent-soft)`, circular highlight |
| Press | `transform: scale(0.9) translateY(1px)` |
| Sending (streaming) | Icon morphs to stop square (already implemented), add gentle pulse animation |

### Doc/provider chip refinement

- `transition: all 150ms`
- Hover: `background: var(--accent-soft)`, `color: var(--accent)`, `translateY(-1px)` lift
- Active: `translateY(0) scale(0.97)` press
- Doc count: `font-family: var(--font-mono)`

### Animated placeholder

When input empty and no active conversation, placeholder text crossfades between:
1. "Ask about your documents..."
2. "Compare sections across files..."
3. "Summarize key findings..."

Implemented with 3 stacked `<span>` elements (absolutely positioned inside a relative container). Each span uses `@keyframes placeholderCycle` with total duration of 12s (4s visible per item):
```
@keyframes placeholderCycle {
  0%, 8%   { opacity: 0 }
  12%, 30% { opacity: 1 }
  38%, 100%{ opacity: 0 }
}
```
- Span 1: `animation-delay: 0s`
- Span 2: `animation-delay: 4s`
- Span 3: `animation-delay: 8s`
- All: `animation: placeholderCycle 12s ease-in-out infinite`

Add `aria-hidden="true"` to all placeholder spans and `aria-label="Message input"` on the textarea for screen readers.

### Files changed
- `src/components/chat/ChatPanel.tsx` — auto-grow behavior, send button states, placeholder spans
- `src/components/chat/ChatPanel.module.css` — all focus/hover/active states, placeholder animation

---

## 4. Page & Panel Transitions

### Slide panels (Docs, Trace)

- **Open:** translateX(100%) to translateX(0), 250ms, `cubic-bezier(0.32, 0.72, 0, 1)` (fast start, soft landing)
- **Close:** translateX(0) to translateX(100%), 200ms (faster out)
- **Backdrop:** opacity 0 to 0.35, 200ms. Increase existing `backdrop-filter: blur(2px)` to `blur(4px)` as a static property (not animated — `backdrop-filter` is not animatable). Animate only the backdrop's `opacity`.
- **Content stagger:** Panel children get `animation-delay` based on their index: header at 0ms, first section at 60ms, second at 120ms, etc. Applied via `nth-child` CSS selectors on `.panelSection` elements: `.panelSection:nth-child(1) { animation-delay: 0ms }`, `.panelSection:nth-child(2) { animation-delay: 60ms }`, up to `:nth-child(5)` (enough for current panels). Each child uses `@keyframes staggerIn { from { opacity: 0; translateY(8px) } to { opacity: 1; translateY(0) } }` with `animation-fill-mode: backwards`.

### Close animation handling

Add intermediate `closing` state **inside** SlidePanel via internal `animState`:
- Parent still controls visibility via a boolean `open` prop (no change to parent logic)
- SlidePanel maintains internal `animState: 'entering' | 'open' | 'exiting' | 'exited'`
- When `open` goes false: set `animState = 'exiting'`, apply `.slideOut` CSS class
- `onAnimationEnd` on the panel container: guard with `if (e.target !== e.currentTarget) return;` to prevent child animation bubbling, then set `animState = 'exited'`, call `props.onExited?.()` callback
- Parent listens to `onExited` to actually unmount the panel (conditional render stays in parent, but keyed on a `mounted` state that lags behind `open` by the animation duration)
- This means: parent sets `open=false` → SlidePanel plays exit animation → `onExited` fires → parent unmounts

### Settings modal

- **Open:** scale(0.97) translateY(6px) to scale(1) translateY(0), 180ms, same cubic-bezier
- **Close:** opacity 1 to 0 + scale(1) to scale(0.98), 120ms
- Same backdrop blur treatment

### Conversation switcher dropdown

- **Open:** scaleY(0.96) opacity(0) to scaleY(1) opacity(1), transform-origin top, 150ms
- **Close:** reverse, 100ms
- Items stagger: each row +30ms delay

### Conversation switching

Crossfade between conversation message lists using CSS `transition` (not `@keyframes`):
- Messages container has `transition: opacity 100ms ease` by default
- When `activeConversationId` changes, set `transitioning=true` → apply `opacity: 0` via `.fadeOut` class
- Listen for `onTransitionEnd` (with `e.propertyName === 'opacity'` guard) → set `transitioning=false` → messages re-render with new content → container transitions back to `opacity: 1`
- Implementation: a `prevConvId` ref tracks the previous conversation. A `useEffect` watching `activeConversationId` triggers the sequence. The `transitioning` state boolean controls the class.
- Prevents jarring instant-swap

### Files changed
- `src/components/common/SlidePanel.tsx` — closing state, onAnimationEnd
- `src/components/common/SlidePanel.module.css` — open/close keyframes, backdrop transitions, content stagger
- `src/components/settings/SettingsModal.module.css` — refined open/close animations
- `src/components/common/ConversationSwitcher.module.css` — dropdown reveal, item stagger
- `src/components/chat/ChatPanel.module.css` — conversation crossfade

---

## 5. Empty States & Personality

### Warm gradient backdrop

- `radial-gradient(ellipse at 50% 40%, var(--accent-soft) 0%, transparent 70%)` behind the logo area
- Dark mode: slightly more visible intensity
- Light mode: very subtle, barely perceptible warmth

### Animated logo

- `@keyframes breathe` — scale(1) to scale(1.04) to scale(1), 4s ease-in-out infinite
- Synced box-shadow pulse: `0 0 20px var(--accent-soft)` to `0 0 40px var(--accent-soft)` and back
- Applied to the existing logo container

### Copy changes

- Heading: "What would you like to explore?"
- Subtitle: "Add documents and ask questions -- I'll navigate the structure to find answers"

### Suggestion chips

Three clickable chip buttons below subtitle:
- "Summarize key findings"
- "Compare across documents"
- "Find specific details"

On click: populate the input bar with that text. Styled as pill buttons with:
- `border: 1px solid var(--border)`, `border-radius: 20px`, `padding: 6px 14px`
- Hover: `border-color: var(--accent)`, `color: var(--accent)`, `background: var(--accent-soft)`
- `@keyframes staggerFadeIn` — each chip appears 100ms after previous

### No-provider state

- "Configure Provider" button: `border: 1px dashed var(--accent)`, `color: var(--accent)`
- `@keyframes borderPulse` — border opacity subtly brightens and dims, 2s infinite

### With-provider-no-docs state

- "Add a document to get started"
- Dashed border drop zone visual
- `@keyframes dashDrift` — background-position shift creating moving dash effect

### Files changed
- `src/components/chat/ChatPanel.tsx` — new empty state markup, suggestion chip handlers
- `src/components/chat/ChatPanel.module.css` — gradient, breathing logo, chip styles, state variations

---

## 6. Micro-interactions & Hover States

### Universal button contract (in theme.css)

Every interactive element gets:
- Hover: `background-color` transition 150ms
- Active/Press: `transform: scale(0.97)`, transition 80ms
- Focus-visible: `outline: 2px solid var(--accent)`, `outline-offset: 2px`

### TopBar icons

- Hover: existing `bg-hover` + `translateY(-1px)` lift
- Active: `scale(0.92)`
- Active panel state: `::after` pseudo-element, 2px bottom accent bar, `scaleX(0) to scaleX(1)` 150ms

### ThinkingBlock step rows

- Hover: `background: var(--bg-hover)` transition
- Expanded: `background: var(--bg-elevated)` persists

### Conversation switcher items

- Hover: left accent bar width 0 to 3px, `background: var(--accent)`, 120ms
- Active conversation: persistent accent bar + bolder text

### Settings modal controls

- Toggle switch: thumb shadow change on hover
- Selects: border-color transition on hover, accent on focus
- Slider thumb: scale(1.15) hover, scale(0.95) active

### SlidePanel close button

- Hover: `transform: rotate(90deg)` 200ms — X icon quarter-turn
- Active: scale(0.9)

### Message bubbles (user)

- Hover: shadow `var(--shadow-sm) to var(--shadow-md)`, 200ms — tangible object feel

### Files changed
- `src/styles/theme.css` — universal button/interactive base styles
- `src/components/common/TopBar.module.css` — icon lift, active indicator bar
- `src/components/chat/ThinkingBlock.module.css` — row hover/expanded states
- `src/components/common/ConversationSwitcher.module.css` — accent bar hover
- `src/components/settings/SettingsModal.module.css` — control hover/focus states
- `src/components/common/SlidePanel.module.css` — close button rotation
- `src/components/chat/ChatPanel.module.css` — user bubble hover

---

## 7. Scrollbar & System Integration

### Custom scrollbars

In `theme.css`, applied globally:

```css
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
::-webkit-scrollbar-thumb:hover { background: var(--text-tertiary); }
scrollbar-width: thin;
scrollbar-color: var(--border) transparent;
```

### Content edge fading

Chat messages container uses a single CSS `mask-image` gradient combining top and bottom fade:
```css
mask-image: linear-gradient(
  to bottom,
  transparent 0%,
  black 24px,
  black calc(100% - 120px),
  transparent 100%
);
```
This single gradient handles both edges without needing `mask-composite`.

### Smooth scroll

- `scroll-behavior: smooth` on messages container
- Auto-scroll on new messages: `scrollTo({ behavior: 'smooth' })`
- "Scroll to bottom" floating pill: appears when scrolled up, `@keyframes fadeInUp`, positioned at bottom center of messages area

### Selection color

```css
::selection {
  background: var(--accent-soft);
  color: var(--text-primary);
}
```

### Tauri window integration

- TopBar already has `-webkit-app-region: drag`
- Add macOS traffic light padding: `padding-left: 78px` conditional on platform
- Frameless window: TopBar IS the title bar

### Files changed
- `src/styles/theme.css` — scrollbar styles, selection color, global smooth scroll
- `src/components/chat/ChatPanel.module.css` — edge fading masks
- `src/components/chat/ChatPanel.tsx` — scroll-to-bottom indicator, smooth scroll logic
- `src/components/common/TopBar.tsx` — macOS padding detection
- `src/components/common/TopBar.module.css` — conditional platform padding

---

## Implementation Order

Phases ordered by dependency (each builds on the previous):

1. **Typography & Theme tokens** — foundation everything else references
2. **Scrollbar & System integration** — global styles, affects all scroll containers
3. **Micro-interactions & Hover states** — universal button contract, then per-component
4. **Input bar refinement** — auto-grow, focus states, chips
5. **Empty states & Personality** — new markup and animations
6. **Thinking & Tool use animation** — 3-dot pulse, completion choreography
7. **Page & Panel transitions** — SlidePanel closing state, stagger animations

---

## Files Summary

| File | Changes |
|------|---------|
| `src/styles/theme.css` | Token updates, prose styles, scrollbar, selection, button contract |
| `src/components/chat/ChatPanel.tsx` | Auto-grow, empty states, suggestion chips, scroll-to-bottom, crossfade |
| `src/components/chat/ChatPanel.module.css` | Prose, input states, empty state, edge fading, crossfade |
| `src/components/chat/ThinkingBlock.tsx` | 3-dot markup, completion classes |
| `src/components/chat/ThinkingBlock.module.css` | Pulse, popIn, stagger, expand keyframes |
| `src/components/common/SlidePanel.tsx` | Internal animState, onExited callback, onAnimationEnd |
| `src/components/common/SlidePanel.module.css` | Open/close keyframes, backdrop, stagger |
| `src/components/common/TopBar.tsx` | macOS padding detection |
| `src/components/common/TopBar.module.css` | Icon lift, active indicator bar |
| `src/components/common/ConversationSwitcher.module.css` | Dropdown reveal, accent bar |
| `src/components/settings/SettingsModal.module.css` | Refined animations, control states |
