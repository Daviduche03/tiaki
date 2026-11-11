# Tiaki — brand colour

Three colours, one of them light. Values are fixed; do not sample new ones from a
mockup.

## Core

| | Hex | Role |
| --- | --- | --- |
| **Paper** | `#E6E0D4` | Default surface. Warm, low-glare, never clinical white. |
| **Ink** | `#151413` | Text, marks, dark surface. 14.00:1 on paper. |
| **Lamp** | `#B36A15` on paper · `#D08A2C` on ink | The beacon's light. Marks one thing at a time. |

The lamp is the only saturated colour in the system. It is not a brand fill and
never a background for large areas — it marks the current generation, the
pending approval, or the focused action.

## Neutrals

| Token | Hex | Use |
| --- | --- | --- |
| `paper-raised` | `#F1ECE3` | Cards, popovers |
| `paper-sunken` | `#D8D1C2` | Wells, inputs, pressed states |
| `ink-raised` | `#1B1917` | Panels on dark |
| `ink-2` | `#5A544A` | Secondary text (5.70:1) |
| `ink-3` | `#665F54` | Tertiary text, metadata (4.80:1) |

Hairlines are ink at 12% on paper, paper at 14% on ink — never a grey value.
Disabled text is ink at 40%.

## State

| Token | Hex | Contrast on paper |
| --- | --- | --- |
| `kept` | `#2E6B4E` | 4.80:1 |
| `refused` | `#A23B2A` | 5.00:1 |
| `lamp-text` | `#8A4E12` | 5.03:1 |

State is semantic, never decorative, and never appears in the mark.

## Measured pairs

| Pair | Ratio | |
| --- | --- | --- |
| Ink on paper | 14.00:1 | AA |
| Ink-2 on paper | 5.70:1 | AA |
| Ink-3 on paper | 4.80:1 | AA |
| Lamp text on paper | 5.03:1 | AA |
| Lamp on paper (graphics) | 3.20:1 | AA large / UI |
| Lamp on ink | 6.60:1 | AA |
| Paper on ink | 14.35:1 | AA |
| Kept on paper | 4.80:1 | AA |
| Refused on paper | 5.00:1 | AA |
| Ink on lamp (chip) | 4.37:1 | AA large |

## Rules

1. **Three colours on screen.** Paper, ink, lamp. A fourth means something is wrong.
2. **The lamp marks one thing.** When everything is lit, nothing is.
3. **State is not brand.** Kept and refused communicate outcome only.
4. **No gradients, no tints.** Depth comes from the two paper steps and hairlines.
5. **Red is not "danger".** Refused is a deep brick and lamp is amber; a system that
   only shouts in red is one you stop hearing.

## Why amber

The mark is a lighthouse. Its light is warm, not the blue every systems product
reaches for, and not the red that consumers already read as "error". Amber on
warm paper also survives long sessions without the glare of pure white or the
flatness of a grey dashboard.
