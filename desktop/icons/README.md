# Tiaki — app icon set

Fourteen icons, one construction. Rendered from SVG at any size; the shell
loads these files directly from `/usr/share/tiaki/icons/`.

## Rules

- **64×64 grid**, tile 0.5→63.5, corner radius 14.
- **Tile:** `#1B1917` with a 1px `#E6E0D4` stroke at 14% — the same surface as
  a raised panel, so the dock reads as one material.
- **Glyph:** `#E6E0D4` only. No second colour, no gradients, no shadows, no
  bevels, no embedded bitmaps.
- **Stroke:** 4.5 on the 64 grid (4 for dense glyphs), round caps and joins.
- **Optical box:** glyphs live inside 14→50; nothing touches the tile edge.
- One filled element at most per glyph, for weight — never for colour.

The lamp `#D08A2C` does not appear in the set. It belongs to the running-app
dot and the approval chip, so the dock's only colour means "this is running".

## Files

| | |
| --- | --- |
| `terminal` | prompt chevron and cursor |
| `files` | folder |
| `editor` | page with fold and text lines |
| `browser` | globe |
| `monitor` | frame with a waveform |
| `agents` | four-point spark |
| `trash` | bin |
| `settings` | gear |
| `clock` | clock face |
| `theme` | half-filled circle |
| `wallpaper` | framed landscape |
| `transactions` | ledger rows with entry markers |
| `reload` | circular arrow |
| `exit` | power symbol |

`archive/` holds the previous icons (Inkscape SVGs with gradients and embedded
PNGs). They are not for use.

Adding an icon: draw it on the same grid, add the file, and reference its name
as `art` in `desktop/desktop.json`. No code change.
