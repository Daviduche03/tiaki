# Tiaki — default wallpapers

Ten defaults: five drawn, five photographic. Both families obey the same rule —
**the wall never out-shines the windows.**

## Drawn (`drawn/`)

| | File | Family | Use |
| --- | --- | --- | --- |
| 01 | `01-ink-field.svg` | ink field | **Default.** Flat near-black, paper grain, one soft light source. No image. |
| 02 | `02-strata.svg` | strata | Settling bands, one ink step at a time, paper hairlines between. |
| 03 | `03-contour.svg` | contour | Topographic rings in ink on paper. The only light family. |
| 04 | `04-horizon.svg` | horizon | A horizon, one paper rule, one 12px lamp square. |
| 05 | `05-columns.svg` | columns | Vertical rules with uneven spacing, faded top and bottom. A ledger's columns, or a palisade. |

All vector, drawn rather than photographed, so they scale to any panel.

## Photographic (`photo/`)

2560×1440, treated, JPEG q88. The treatment is baked in — no runtime filter.

| | File | Subject | Source | Licence |
| --- | --- | --- | --- | --- |
| 01 | `01-coast-fog.jpg` | fog over a shore | Annie Spratt (Unsplash) | CC0 |
| 02 | `02-lake-mist.jpg` | lake and ridge in mist | BLM Alaska | Public domain |
| 03 | `03-mist-pines.jpg` | mist through conifers | Patrick Fore (Unsplash) | CC0 |
| 04 | `04-fog-ridge.jpg` | treeline in fog | Milo McDowell (Unsplash) | CC0 |
| 05 | `05-highland-mist.jpg` | cloud sea over a ridge | mypubliclands | Public domain |

Every file is CC0 or public domain, so shipping defaults never requires an
attribution screen. If a CC BY or BY-SA image is ever added, it must be recorded
here with artist and source, and it cannot be shipped as a default.

### Treatment recipe

```
cover-crop 16:9 · grayscale blend .80
brightness .64 · contrast .90
warm blend 14% toward #4A3E2C
radial ink veil: rgba(21,20,19,.30) centre → .58 edge
grain 12% at sigma 26
```

The veil is the contract: it puts the brightest pixel of every photograph below
paper `#E6E0D4`, so a paper window is always the brightest thing on screen.

### What qualifies

- Weather, not postcards. Fog, overcast coast, still water, stone.
- **No focal subject in the middle 60%** — no boat, no house, no figure. The lake
  image was re-cropped to remove a canoe for exactly this reason.
- No faces, no text, no signage.
- No sunsets, no turquoise water, no HDR.

## Rules for any default

1. No high-frequency detail in the middle 60%.
2. No text, logos, or faces.
3. Contrast budget: the windows are the brightest or darkest thing on screen.
4. One light source at most, small, off-centre.
5. No decorative gradients — grain and banding are material, not glow.
6. No blue.

## Technical

- Drawn: SVG masters, lossless at any size.
- Photographic: 2560×1440 master. Export 1920×1080 for smaller panels.
- One wallpaper per output; never span a single image across monitors.
- Shell chrome (bar, dock) stays ink on every wallpaper, including Contour, so the
  shell never changes colour when the wall does.

## Theme pairing

| Theme | Defaults |
| --- | --- |
| Paper (default UI) | 01, 02, 04, 05 drawn · all five photographs |
| Ink (dark UI) | 03 contour · 02 strata |

Contour is excluded from the paper UI because paper windows on paper walls lose
their edges.
