# EZ Carpentry

Parametric cabinet design → cut list → CNC files. A Cabinet Vision–style tool with a Rust core
that runs natively (CLI) and in the browser (WebAssembly, with a pure-JS fallback).

```
crates/core   model, part generation, System-32 boring, MaxRects nesting, exporters
crates/cli    `casework` command line
crates/wasm   browser bindings
web/          editor: 3D view, parts, sheets, hardware → web/dist/index.html (one file, works offline)
examples/     kitchen.json
```

## Online

GitHub Pages: https://taras-galper.github.io/EZ_Carpentry/ (enable Settings → Pages → Source: GitHub Actions).
`web/dist/index.html` is the same app as one file; it also works opened locally.

## Downloads

Every push builds `casework` for Windows, Linux and macOS: Actions → latest run → Artifacts.

## Editor

Drag a cabinet in 3D to move it (10 mm grid; its footprint snaps to walls and to the edges of other cabinets, including perpendicular runs). R rotates the selected cabinet 90° in place. Ctrl+Z / Ctrl+Y undo and redo every edit.
Room width/depth/height (Project) draws the back and side walls; back wall at z = 0, side walls at x = 0 and x = width.
The project is kept in the browser between visits; Save writes `project.json`.

## Quoting

Quote tab: rates (markup, VAT, labour per hour, CNC minutes per sheet, assembly and install hours per cabinet, edge band per metre), price per sheet for each material, and unit price per hardware item.
Cost = whole sheets used × sheet price + edge band + hardware + labour. Each cabinet and each worktop is priced at its share of that cost (material by part area, CNC time by area, hardware and assembly by cabinet) plus markup; VAT is added to the subtotal.
Prices are in `project.json` under `pricing`; sample prices are placeholders.

## Build

```
./build.sh
```

## CLI

```
casework sample > kitchen.json
casework summary kitchen.json
casework export kitchen.json -o out/          # folder of files
casework export kitchen.json --zip kitchen.zip
```

## Exports

| File | Contents |
|---|---|
| `project.json` | Full project; reopen in the editor or feed to the CLI |
| `cutlist.csv` | Cabinet, part, material, thickness, length (with grain), width, qty, edge banding L1 L2 W1 W2, bores |
| `hardware.csv` | Hinges, slides, shelf pins, handles per cabinet + totals |
| `quote.html` | Printable customer quote: one line per cabinet and worktop, subtotal, VAT, total |
| `quote.csv` | Internal costing: sheets, edge band, hardware, labour, markup, item prices, totals |
| `labels.csv` | One row per physical part: id, cabinet, size, edges, bores, cut-outs, sheet number and position |
| `parts/*.dxf` | One DXF R12 per part, inside face up, 1 unit = 1 mm |
| `sheets/*.dxf` | Nested sheet layouts, ready for CAM |
| `sheets/*.svg` | Printable sheet layouts |
| `drawings/*.svg` | Shop drawing per cabinet: front elevation, section, plan, dimensioned |
| `model.obj` | 3D model of all parts (mm, Y up) |

DXF layers (map these to tools/depths in AlphaCAM or any CAM):

| Layer | Meaning |
|---|---|
| `OUTLINE` | Part profile, cut through |
| `CUTOUT` | Inner through profile (sink / hob), rounded corners as true arcs |
| `DRILL_D5_Z12` | Ø5 bore, 12 deep (shelf pins, hinge plates, slides) |
| `DRILL_D35_Z13` | Ø35 hinge cup, 13 deep |
| `DRILL_D7_THRU` | Ø7 through (confirmat), `DRILL_D5_THRU` handle holes |
| `SHEET` | Sheet boundary (sheet files) |
| `LABEL` | Part names (sheet files) |

## Construction rules

Frameless / System 32, applied back, full-overlay fronts.

- Sides full height (toe-kick notch on base/tall), bottom and top between sides; base cabinets get two 100 mm top rails.
- Back = full width × (height − toe kick), applied to the rear; sides and shelves are shortened by its thickness.
- Drawer fronts stack from the top using the heights you list; doors fill the rest (0, 1 or 2 leaves).
- Hinges: 2 up to 900 mm, 3 to 1600, 4 to 2000, then 5; cups at the hinge-cup inset, plates on the system line.
- Slides: longest of 250–550 mm that fits; drawer box = interior − 2 × slide clearance.
- Shelf-pin rows at the system line from front and back, 32 mm pitch; shelves snap to the nearest row.
- Worktop: one per straight run of touching base cabinets along their own axis (any rotation), deepest cabinet + overhang. At an L corner the later run is trimmed to butt against the first. Set thickness 0 to turn off.
- Worktop cut-outs: set Sink or Hob on a base cabinet; the cut-out is centred over it, set back from the worktop front. Sizes of 0 use 760 × 440 R10 (sink) and 560 × 490 R5 (hob). A warning appears when a cut-out comes within 50 mm of a worktop edge or is wider than the cabinet interior.
- Blind corner: the blind side of the front is a fixed panel (blind width) that the return leg covers; one door fills the rest, hinged away from the blind side. Drawers are not built in blind corners.
- Shaker fronts: two full-height stiles, two rails with tenons (= groove depth), and a panel from the cabinet's panel material. Bores go to whichever piece holds them; fronts smaller than 2 × frame + 60 stay slab.
- Nesting: MaxRects best-short-side-fit with kerf and trim; grained materials keep grain along the sheet length.
