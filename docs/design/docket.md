# Docket — the Tiaki design language

> The OS keeps receipts. The interface looks like the receipt.

Status: proposed, 2026-11-09. Supersedes the ad-hoc palette in `ui/shell/css/`.

---

## 1. Why this exists

Our first three login directions and six setup screens scored 9 of 16 on the
published AI-design-slop rubric (Krebs, 1,590 Show HN pages): Inter everywhere,
blue-black + lavender, glassmorphism, colored glows, gradient avatars, a
centered hero with a badge above it, identical icon cards, all-caps eyebrows,
emoji glyphs. Nothing was broken. Nothing was chosen.

A design system is not a palette. It is a set of decisions we refuse to
re-make on every screen. This document is the refusal list.

## 2. The one idea

Every privileged act in this OS produces a docket entry: who, what, when,
which generation, reversible. The UI should look like that artifact — a
printed record, not a dashboard, not a landing page.

That gives us one strong primitive, repeated until it becomes the signature:
**the row**. A label, a hairline, a machine fact in mono. Everything else is
built from rows.

## 3. Principles

1. **Machine facts are monospaced, and lowercase.** Principals, generations,
   hashes, times, counts, paths. `gen a`, `user:ada`, `tx 8f3a…c1`,
   `2h ago`. Never uppercase-transformed.
2. **One accent, and it means authority.** Vermilion marks the act of
   committing: sign in, approve, finish. If red appears, something is about
   to be written. It is never decoration, never a brand fill, never a chart
   color.
3. **Structure is a hairline, not a card.** 1px rules separate things. Cards
   are reserved for choices, and are a luminance step, not a border + shadow
   + radius stack.
4. **Type carries hierarchy, not color.** Contrast comes from size and
   weight. Color is for meaning only.
5. **No glass, no glow, no gradient.** Elevation is three luminance steps.
   There is exactly one shadow in the system, for true overlays.
6. **Density is a feature.** This is a control plane. Rows are 44px, compact
   rows are 32px, and we do not pad things out to look friendly.

## 4. Tokens

### Color

| Token | Value | Use |
| --- | --- | --- |
| `--ink-0` | `#0c0c0d` | window base, full-bleed surfaces |
| `--ink-1` | `#131314` | panels, sidebars |
| `--ink-2` | `#1a1a1b` | raised: menus, popovers, selected rows |
| `--ink-3` | `#232324` | controls: inputs, buttons, segments |
| `--line` | `rgba(242,239,233,.09)` | structural hairlines |
| `--line-strong` | `rgba(242,239,233,.16)` | focus, hover, active edges |
| `--text` | `#f2efe9` | primary ink (warm, not blue-white) |
| `--text-2` | `#a29e97` | secondary, labels |
| `--text-3` | `#6b6862` | tertiary, placeholders, disabled |
| `--stamp` | `#e2483d` | authority: commit actions only |
| `--stamp-hover` | `#ef5a4e` | |
| `--stamp-press` | `#c93b31` | |
| `--ok` | `#6aa889` | committed / verified / active generation |
| `--warn` | `#c9a227` | pending approval |
| `--err` | `#e2483d` | failed transaction |

Warm neutrals only. No blue-black (`#16161e`), no lavender (`#7aa2f7`), no
coral-pink (`#ef6f5c`) — those are the defaults we are replacing.

### Type

Two families, both shipped: **IBM Plex Sans** (UI) and **IBM Plex Mono**
(every machine fact). Plex is a technical grotesque with actual drawing
decisions in it; Inter is the LLM default and is banned.

| Role | Size / line | Weight | Tracking | Family |
| --- | --- | --- | --- | --- |
| display | 64 / 0.98 | 500 | -0.035em | sans |
| title | 30 / 1.10 | 500 | -0.02em | sans |
| heading | 19 / 1.30 | 500 | -0.01em | sans |
| body | 15 / 1.55 | 400 | 0 | sans |
| ui | 13 / 1.40 | 450 | 0 | sans |
| label | 12 / 1.40 | 500 | 0 | sans |
| meta | 11.5 / 1.40 | 400 | 0.01em | mono |
| value | 12.5 / 1.40 | 400 | 0.01em | mono |

Numbers that change (times, counts, hashes) are always `font-variant-numeric:
tabular-nums`.

### Space

4px base: `4 8 12 16 24 32 48 64 96`.
Section rhythm: 32 between groups, 16 within, 8 between a label and its value.
Page margin at 1440: 96. Content max width: 720 for prose, 1080 for tables.

### Radius

| Token | Value | Use |
| --- | --- | --- |
| `--r-control` | `2px` | buttons, inputs, checkboxes, markers |
| `--r-surface` | `3px` | menus, popovers, cards |
| `--r-pill` | `999px` | only for true status pills, never for buttons |

Concentric rule: outer radius = inner radius + padding. A 2px control inside
a 3px surface with 8px padding is wrong; either pad more or square the inner.

### Elevation

| Level | Background | Edge |
| --- | --- | --- |
| 0 | `--ink-0` | none |
| 1 | `--ink-1` | 1px `--line` |
| 2 | `--ink-2` | 1px `--line-strong` |

Overlays only (menu, launcher, modal, switcher): `--ink-2`, 1px
`--line-strong`, `0 24px 60px rgba(0,0,0,.5)`. That is the entire shadow
library. No glow, no colored shadow.

### Motion

| Token | Value |
| --- | --- |
| `--ease` | `cubic-bezier(0.2, 0, 0, 1)` |
| press | 120ms |
| hover / state | 160ms |
| surface enter | 200ms |
| press scale | `0.98` (never 0.95 or below) |

Nothing on the login screen animates. Nothing keyboard-initiated animates.
`prefers-reduced-motion` removes movement, keeps opacity.

### Icons

One set, 1.5px stroke, 16px and 20px, `currentColor`, square caps, no fills
except active state. Emoji and text glyphs (⏻ ◉ ⌁ ↵) are banned as icons.

## 5. Primitives

### Row — the signature

```
label                                  value        ← sans 15 / mono 12.5 --text-2
────────────────────────────────────────────────────  ← 1px --line
```

44px tall, 24px horizontal inset, hairline bottom, no vertical hairlines.
Optional 6px square state marker at the left (2px radius, `--ok` / `--warn`).
Rows replace cards for lists, settings, transactions, setup review, agents.

### Rail — generation state

A 1px horizontal rule with tick marks every 24px; the active generation is a
filled 8px square, the standby is a hollow 8px square. Used on login, setup,
and the bar. This is our only ornament, and it encodes real state.

### Stamp — the single commit action

Red fill, `--r-control`, 44px tall, label 14/500, no glow, no gradient,
`scale(0.98)` on press. One per screen. Secondary actions are text buttons
with no fill.

### Field

Underline, not a filled box: 1px `--line-strong` bottom rule, 40px tall,
label above in `label` style, placeholder `--text-3`. Focus raises the rule
to `--stamp` and adds no glow.

### Status

A word plus a 6px square. `committed` (ok), `pending` (warn), `failed` (err),
`standby` (text-3). Never a glowing dot, never a pill with a gradient.

## 6. Layout

- 12-column grid, 24px gutters, 96px margins at 1440.
- One vertical axis. Left-aligned by default; centered composition is a
  deliberate exception, not a default.
- Asymmetry is allowed and preferred: a 5/7 split beats a centered stack.
- Full-bleed wallpaper only where it is the subject (desktop, lock). Panels
  are solid ink; we do not blur the wallpaper behind UI.

## 7. Voice

- Lowercase for machine facts, sentence case for prose.
- Name the transaction, not the feeling: "4 changes, all reversible" not
  "You're all set!".
- No exclamation marks. No "Let's get started". No em-dash-heavy marketing.
- Every confirmation states what changed and how to undo it.

## 8. Banned

Hard bans, checked in review:

- Inter, Space Grotesk, Instrument Serif; serif italic accent words.
- Blue-black backgrounds, lavender/purple accents, coral-pink brand fills.
- Gradients of any kind on UI surfaces, text, avatars, or logos.
- Colored glows, colored shadows, neon status dots.
- `backdrop-filter` / glass on UI chrome.
- Emoji or Unicode glyphs as icons.
- Centered hero + badge above the headline.
- Identical icon-topped cards as a layout pattern.
- All-caps section labels (mono lowercase is the replacement).
- Colored left borders on rows or cards (red rule on the *left* of a
  selected row is banned; use the state marker instead).
- `border-radius: 12px` applied uniformly to everything.
- Three type sizes within 2px of each other.

## 9. Adoption

1. IBM Plex Sans/Mono ship in the image (pinned IBM releases — Debian does
   not package them); no `fonts-inter` dependency remains.
2. Replace `ui/shell/css/base.css` with `tokens.css` + a reset.
3. Port in this order: bar → dock → menus → launcher → Settings shell →
   Settings pages → Control Center → switcher → agents.
4. Each port is one transaction per module, so the change is attributable
   and revertible like everything else.
5. Login and first-run setup are built on this language from day one, not
   retrofitted.
