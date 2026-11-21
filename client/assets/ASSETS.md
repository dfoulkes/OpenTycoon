# Client assets

Stand-in art for the game inspector (spec §10d, R89): free CC0 low-poly models only, each kit
with its licence beside its files. Only the files the client uses are kept here.

| Folder | Files | Source | Licence |
|---|---|---|---|
| `kenney/starter-kit-city-builder/` | `building-small-a.glb` … `building-small-d.glb`, `Textures/colormap.png` | Kenney, [Starter Kit City Builder](https://github.com/KenneyNL/Starter-Kit-City-Builder) (`models/`), see also [kenney.nl](https://kenney.nl) | CC0 1.0 ([`License.txt`](kenney/starter-kit-city-builder/License.txt)) |

The world scene tints each building model with its owner's colour (`scripts/world/lot_view.gd`)
and scales it to the lot's floors. If a model is missing it falls back to a grey box.

Adding an asset: keep it CC0 (or equally free), put its licence file beside it, and add a row
here with the source URL.
