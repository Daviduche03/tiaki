# Tiaki — the mark

**The beacon.** A banded lighthouse: it does not stop the ship, it makes the rocks
visible. That is what guarded mode does — it does not lock you out of your own
machine, it shows you what a change will touch before it happens.

Chosen 2026-11-09. Superseded studies are in `archive/` and are not for use.

## Files

| File | Use |
| --- | --- |
| `tiaki-mark.svg` | The mark. 100×100, `currentColor`. Default everywhere ≥16px. |
| `tiaki-mark-small.svg` | Two-band cut for ≤16px. Same silhouette, wider gaps, so the bands survive. |
| `tiaki-lockup.svg` | Mark + `Tiaki` wordmark, outlined. Use when both appear together. |
| `tiaki-icon.svg` | 1024 app icon: ink mark on paper, 228 radius. |

## Construction

- 100×100 grid. Ink bounds x 21–79, y 7–96.
- The tower is three separate trapezoids. The gaps are **real holes**, not
  background-coloured bars — the mark must work on any surface.
- Geometry derives from one taper: x 38→28 on the left, 62→72 on the right,
  between y 39 and 88.
- The plinth is 58 wide and 8 tall, and sets the baseline the mark stands on.

## Colour

Two colours only:

| | Value |
| --- | --- |
| ink | `#151413` |
| paper | `#E6E0D4` |

In UI, use `currentColor` so the mark takes the surrounding ink. Never vermilion:
red is reserved for authority actions, not for identity.

Note: `currentColor` only inherits when the SVG is **inlined** in the document or
recoloured by the loader. An `<img src="tiaki-mark.svg">` will not pick up the
parent's colour — inline it, or rasterise it with the ink/paper value baked in.

## Sizes

- **≥ 16px** — `tiaki-mark.svg`
- **≤ 16px** — `tiaki-mark-small.svg` (two bands)
- **< 12px** — omit the mark, set the wordmark alone.

Optical sizes: 64 bar, 32 menus, 20 sidebar, 16 lists, 12 favicon. In running
text the name is always written **Tiaki**.

## Clear space

Half the mark's height on every side. Nothing enters that box, including text
and rules.

## Don'ts

- No gradient, shadow, glow, outline, stroke, or bevel.
- No rotation, skew, or perspective.
- Do not add a light beam, rays, or windows — the bands are the mark.
- Do not fill the gaps to make a solid tower.
- Do not place the mark on a busy photograph. If the surface is not flat, put it
  on a paper or ink field first.
- Do not recolour it per theme; ink or paper only.

## Wordmark

`Tiaki`, IBM Plex Sans 500, tracking -0.03em, **capital T**. It is a proper
noun; the lowercase wordmark was a web-brand tic, not a decision.

In `tiaki-lockup.svg` the wordmark is converted to outlines so it cannot fall
back to a different font. Lockup: the mark's ink height equals the cap height
(72), the mark's plinth sits on the shared baseline, gap 26.

## What this establishes

The mark is the first artefact of the new material language: **ink on paper,
solid forms, hard edges, one colour, weight instead of glass**. The interface
system is derived from this, not the reverse.
