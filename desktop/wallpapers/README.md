# Wallpapers

Ten defaults: five drawn, five photographic. Shipped to
`/usr/share/tiaki/wallpapers/` by the installer (`make payload` copies this
directory verbatim). Spec: `docs/design/wallpapers/README.md`.

`drawn-01-ink-field.jpg` is the first-boot default. It is also the fallback in
`tiaki-wallpaper.sh`, `launcher.rs`, and `settings/data.rs` when the saved
choice is missing — keep all four in sync.

## Drawn (SVG masters in `docs/design/wallpapers/drawn/`)

| File | Family |
|---|---|
| `drawn-01-ink-field.jpg` | ink field (default) |
| `drawn-02-strata.jpg` | strata |
| `drawn-03-contour.jpg` | contour |
| `drawn-04-horizon.jpg` | horizon |
| `drawn-05-columns.jpg` | columns |

## Photographic (treated, baked in)

| File | Subject | Source | License |
|---|---|---|---|
| `photo-01-coast-fog.jpg` | fog over a shore | Annie Spratt (Unsplash) | CC0 |
| `photo-02-lake-mist.jpg` | lake and ridge in mist | BLM Alaska | Public domain |
| `photo-03-mist-pines.jpg` | mist through conifers | Patrick Fore (Unsplash) | CC0 |
| `photo-04-fog-ridge.jpg` | treeline in fog | Milo McDowell (Unsplash) | CC0 |
| `photo-05-highland-mist.jpg` | cloud sea over a ridge | mypubliclands | Public domain |

All CC0 or public domain, so no attribution screen is needed.

1920×1080 JPEG, q88–90. The Settings picker and launcher backdrop list
whatever `*.jpg` files are in this directory, so adding a wallpaper is adding a
file — no code change.
