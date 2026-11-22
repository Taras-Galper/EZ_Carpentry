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

Every push builds `casework` for Windows and Linux: Actions → latest run → Artifacts. For macOS, start the workflow by hand (Actions → build → Run workflow).

## Editor

**Room.** *Draw room* (top of the left panel; switches to Plan) and click the inside corners (snaps to a 10 mm grid and 45° directions; Backspace removes the last corner); click the first corner, double-click, or press Enter to finish. *Rectangle* starts from a box. Drag the square handles to move corners. Click a wall's length label (or the wall) to select it: set its length, add doors and windows, or **fill it with cabinets** (base, wall, or both). Filling packs standard widths (1000, 900, 800, 600, 500, 450, 400, 300), keeps clear of doors, windows, corners and cabinets already there, and closes gaps against walls or cabinets with a filler (gaps beside doors and windows stay open). The same layout runs from the CLI (`casework fill`).

**DXF floor plan.** *DXF plan* (or `casework import-room`) reads LINE, LWPOLYLINE and POLYLINE outlines (arcs, text and dimensions are ignored), joins lines into closed loops, and takes the largest — or, when walls are drawn with both faces, the inside face. Units come from `$INSUNITS`, else are guessed from the size (m / cm / mm). Doors and windows are added afterwards on the walls.

**Projects, presets, defaults.** Projects are kept in this browser (*Projects*: open, copy, delete); *Save* / *Open* move them as `project.json` files. ★ on a cabinet saves it as a preset under *My cabinets*. *Save as shop defaults* (Project panel) stores materials, prices, CNC tools and construction settings for every new project. Materials can be added, and deleted when no cabinet uses them.

**Checks.** Warnings (status bar, packet cover) for cabinets overlapping at the same height, cabinets through a wall, and doors hinged hard against a side wall (they need a filler). *Fill* leaves a 50 mm filler where a run meets a side wall for the same reason.

**Moving.** Select a cabinet to get a move gizmo at its bottom-front centre; its arrows follow the cabinet's own width, depth and height, in 10 mm steps. The X / Y / Z / ↻ box under the 3D view takes exact values, or `+n` / `-n` to move by n (`=-n` for a negative coordinate). Arrow keys nudge 10 mm (Shift: 100 mm). Dragging a cabinet near any wall, angled or not, turns it to face the room and backs it onto the wall.

**Phones.** Below 760 px wide the view fills the screen; *Cabinets* and *Edit* at the bottom open slide-up panels (the add buttons and cabinet list; properties of the selected cabinet or wall), *⋯* holds New / Open / Save and the exports, and 3D / Plan / Front float over the view. One finger rotates in 3D and pans in Plan / Front, two fingers zoom and pan. Room drawing works by tapping; tapping near the first corner closes the room.

`window.ezCarpentry` exposes the current project, selection, view and a world-to-screen helper for scripts and tests.

3D / Plan / Front switch the camera; Plan and Front are flat (orthographic) for layout, left-drag on empty space pans. In Front, dragging moves a cabinet sideways only.
Drag a cabinet in 3D to move it (10 mm grid; its footprint snaps to walls and to the edges of other cabinets, including perpendicular runs). R rotates the selected cabinet 90° in place. Ctrl+Z / Ctrl+Y undo and redo every edit.
Room width/depth/height (Project) draws the back and side walls; back wall at z = 0, side walls at x = 0 and x = width.
The project is kept in the browser between visits; Save writes `project.json`.

## G-code

One program per nested sheet, plain ISO / Fanuc-style (`G21 G90`, `T M6`, `S M3`): check it against your machine's post and simulate before the first cut.

- Origin at the sheet's lower-left corner, Z0 on top of the material; through-cuts go `through` mm into the spoilboard.
- Order: vertical drilling per tool → pockets and grooves (narrow ones with the groove cutter; grooves that run out through a part's edge are cut right through it) → holes larger than any drill as helices with the router → sink / hob cut-outs → part profiles, smallest parts first, offset by the cutter radius, in depth passes, with holding tabs on small parts.
- Not in the sheet program, listed in its header: edge bores (edge borer / horizontal aggregate, see the part DXFs) and opposite-face pockets (`nc/face2/<part>.nc`, part flipped with its lower-left corner at the origin).
- Tools, feeds, rpm, pass depth, tab size and safe Z are under Project → CNC. Parts are nested at least one router diameter apart (or the kerf, if larger) so neighbouring paths never cut into each other.

## Quoting

Quote tab: rates (markup, VAT, labour per hour, CNC minutes per sheet, assembly and install hours per cabinet, edge band per metre), price per sheet for each material, and unit price per hardware item.
Cost = whole sheets used × sheet price + edge band + hardware + labour. Each cabinet and each worktop is priced at its share of that cost (material by part area, CNC time by area, hardware and assembly by cabinet) plus markup; VAT is added to the subtotal.
Prices are in `project.json` under `pricing`; sample prices are placeholders.

## Build

```
./build.sh
node web/test/smoke.mjs   # browser check of web/dist on both engines (needs Playwright + Chrome)
```

Rust code is formatted with `cargo fmt` (settings in `rustfmt.toml`); CI checks it.

## CLI

```
casework sample > kitchen.json
casework summary kitchen.json
casework walls kitchen.json                       # list walls with lengths and rotations
casework fill kitchen.json 2 both                 # fill wall 2 with base + wall cabinets (base | wall | both)
casework import-room kitchen.json plan.dxf         # room outline from a DXF floor plan
casework export kitchen.json -o out/          # folder of files
casework export kitchen.json --zip kitchen.zip
```

## Exports

| File | Contents |
|---|---|
| `project.json` | Full project; reopen in the editor or feed to the CLI |
| `cutlist.csv` | Cabinet, part, material, thickness, length (with grain), width, qty, edge banding L1 L2 W1 W2, bores |
| `hardware.csv` | Hinges, slides, shelf pins, handles per cabinet + totals |
| `shop_packet.html` | Printable job packet (A4): cover with totals, materials, construction and hardware checklist; cabinet list; cut list per material with tick boxes and sheet numbers; sheet layouts (landscape); shop drawings with per-cabinet hardware |
| `labels.html` | Part labels, 3 × 8 per A4 (70 × 36 mm): part id, sheet, size along the grain, material, cabinet, edges, bores, a mini drawing and a Code 128 barcode of the part id. Print at 100 % |
| `quote.html` | Printable customer quote: one line per cabinet and worktop, subtotal, VAT, total |
| `quote.csv` | Internal costing: sheets, edge band, hardware, labour, markup, item prices, totals |
| `labels.csv` | One row per physical part: id, cabinet, size, edges, bores, cut-outs, sheet number and position |
| `parts/*.dxf` | One DXF R12 per part, inside face up, 1 unit = 1 mm |
| `sheets/*.dxf` | Nested sheet layouts, ready for CAM |
| `sheets/*.svg` | Printable sheet layouts |
| `drawings/*.svg` | Shop drawing per cabinet: front elevation, section, plan, dimensioned |
| `model.obj` | 3D model of all parts (mm, Y up) |
| `nc/*.nc` | G-code per sheet (see below) |
| `nc/face2/*.nc` | G-code for opposite-face pockets, per part |

DXF layers (map these to tools/depths in AlphaCAM or any CAM):

| Layer | Meaning |
|---|---|
| `OUTLINE` | Part profile, cut through |
| `CUTOUT` | Inner through profile (sink / hob), rounded corners as true arcs |
| `DRILL_D5_Z12` | Ø5 bore, 12 deep (shelf pins, hinge plates, slides) |
| `DRILL_D35_Z13` | Ø35 hinge cup, 13 deep |
| `DRILL_D7_THRU` | Ø7 through (confirmat), `DRILL_D5_THRU` handle holes |
| `DRILL_D8_Z12` | Ø8 dowel bores in sides (dowel joinery) |
| `HDRILL_D{d}_Z{z}` | Horizontal bore into an edge, mid-thickness: a line from the edge inwards, as long as the bore is deep (confirmat pilot Ø5 × 45, dowel Ø8 × 25) |
| `POCKET_Z{z}` | Pocket / groove z deep in the drawn face (back-panel and drawer-bottom grooves) |
| `F2_POCKET_Z{z}` | Pocket on the opposite face: flip the part (routed Shaker recess on the outside of a front) |
| `SHEET` | Sheet boundary (sheet files) |
| `LABEL` | Part names (sheet files) |

## Construction rules

Frameless / System 32, full-overlay fronts. Parts are drawn inside face up (hinge cups, shelf pins, grooves on the drawn face).

- Joinery (Project): confirmat — Ø7 through the side, Ø5 × 45 pilot into the panel edge — or dowels — Ø8 × 12 in the side, Ø8 × 25 into the edge. Every joint gets both halves, lined up.
- Back (Project): applied over the back edges, or in a groove — sides, bottom and top (or back rail) are full depth and grooved (back inset from the back edge, groove depth), back panel = interior + 2 × groove depth − 1 mm.
- Drawer boxes: bottom in a groove 10 mm up in sides, front and back; bottom = inside + 2 × groove depth − 1 mm.

- Sides full height (toe-kick notch on base/tall), bottom and top between sides; base cabinets get two 100 mm top rails.
- Back = full width × (height − toe kick), applied to the rear; sides and shelves are shortened by its thickness.
- Drawer fronts stack from the top using the heights you list; doors fill the rest (0, 1 or 2 leaves).
- Hinges: 2 up to 900 mm, 3 to 1600, 4 to 2000, then 5; cups at the hinge-cup inset, plates on the system line.
- Slides: longest of 250–550 mm that fits; drawer box = interior − 2 × slide clearance.
- Shelf-pin rows at the system line from front and back, 32 mm pitch; shelves snap to the nearest row.
- Worktop: one per straight run of touching base cabinets along their own axis (any rotation), deepest cabinet + overhang. At an L corner the later run is trimmed to butt against the first. Run ends cover end panels; free-standing ends (no wall or cabinet beside them) also get the end overhang. Set thickness 0 to turn off.
- Worktop cut-outs: set Sink or Hob on a base cabinet; the cut-out is centred over it, set back from the worktop front. Sizes of 0 use 760 × 440 R10 (sink) and 560 × 490 R5 (hob). A warning appears when a cut-out comes within 50 mm of a worktop edge or is wider than the cabinet interior.
- Dishwasher: integrated decor panel (slab or Shaker) and toe kick; the machine is shown in 3D but not cut. Joins worktop runs.
- Appliance space (fridge, freestanding oven): holds the space, cuts nothing, not charged. Under-worktop height (< 1000 mm on the floor) joins worktop runs; taller ones end them.
- Filler: front-material strip plus toe kick; joins worktop runs when under-worktop height.
- Plinth (Project): toe-kick board fixed to notched sides, or legs + plinth — sides stop above the plinth (holes move with them), 4 adjustable legs (6 over 900 mm wide) and 2 plinth clips per cabinet.
- Wardrobe (tall cabinet, Hanging rail): fixed hat shelf (hat-shelf gap below the top), joined like a bottom; Ø25 rail 60 mm under it with two supports; adjustable shelves go below the hat shelf.
- Finished back (islands): front-material panel over the back, covering end panels too.
- End panels: finished side in front material outside the carcass, deep enough to cover the door edge, full height.
- Blind corner: the blind side of the front is a fixed panel (blind width) that the return leg covers; one door fills the rest, hinged away from the blind side. Drawers are not built in blind corners.
- Appliance opening (base and tall; *Oven* preset): an open slot of the given height and height from the floor between two fixed shelves joined like a bottom (the bottom or top stands in when the slot reaches it). Drawers stack down from the opening, doors fill what is left below it, and door(s) of the same count close the space above, where the adjustable shelves go.
- L corner (Base or Wall, Corner = L shape): fills a room corner with two arms — the cabinet's width along one wall, the return width along the other, both at the cabinet's depth. L-shaped bottom, top and shelves (front edges banded; a support bracket at the back corner), a back on each wall, a left side and an end side, two toe kicks, and a bi-fold door pair: one leaf hinged on the left side, the other folding onto it with 170° hinges. Backs are applied; drawers and finished backs are not built. Worktops of both walls run over it. *L corner* / *Wall L* drop it into the first free inside corner of the room.
- Routed Shaker: one-piece MDF front with the recess (frame width, routed depth) on the outside face; machine it as a second-face operation.
- Shaker fronts: two full-height stiles, two rails with tenons (= groove depth), and a panel from the cabinet's panel material. Bores go to whichever piece holds them; fronts smaller than 2 × frame + 60 stay slab.
- Nesting: MaxRects best-short-side-fit with kerf and trim; grained materials keep grain along the sheet length.
