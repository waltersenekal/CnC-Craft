<p align="center">
  <img src="images/icon.png" width="128" alt="CncCraft icon">
</p>

<h1 align="center">Cnc Craft</h1>

<p align="center">
  <b>Design, toolpaths, G-code and machine control for CNC routers in one desktop app.</b><br>
  Draw or import a design, choose how each part is cut, preview the result in 2D and 3D,
  check the job for mistakes, then save the G-code or run it on your machine.
</p>

![CncCraft with a design and its toolpaths](images/workspace.png)

---

## Features

### Design
- **Drawing tools:** lines, rectangles, ellipses and circles, polygons, stars, text in any installed font, drill points, measuring and dimension lines.
- **Editing:** point editing, trim, knife, shape builder, round and chamfered corners, weld, subtract, intersect, exclude, offset, outlines, join, break apart, simplify.
- **Selecting:** double-click a line to take its whole touching outline in one go, then Ctrl+G to group it; Shift adds the run to the current selection.
- **Precise placement:** a number bar for position, size, rotation and scale, plus snapping to the grid and to other objects, ruler guides, smart guides, symmetry drawing, alignment and spacing, grid and circular arrays, and nesting to fit parts on the stock with little waste.
- **Live effects:** offset, rounded corners, warp, envelope, simplify, smooth and radial repeat stay editable, so you can change them later.
- **Modify tools:** warp, smooth, hatch fill, lattice, dot and hole templates, knockouts and inlays, stencil bridges, shadows, radial repeat, blend, clip, copy along a path and fit into a shape.
- **Makers:** finger-jointed boxes, gears, living hinges, jigsaw puzzles, signs, bolt circles, hole grids and QR codes.
- **Shape library:** ready-made shapes, plus your own folder of SVG and DXF files.
- **Tiles and rubber stamps:** split a large design into jointed tiles, or turn artwork into a mirrored stamp.

### Toolpaths
| Cutting | Carving | Special |
|---|---|---|
| Profile (outside, inside, on the line) | V-Carve | Drag knife |
| Pocket (offset or raster) | V-Carve inlay (pocket and plug) | Thread milling |
| Drill (with pecking) | Prism (raised bevel) | Keyhole slots |
| Bore (spiral holes) | Chamfer | Moulding (a profile swept along rails) |
| Dogbone slot | Fluting | Texture |
| Surfacing | 3D rough and 3D finish | |

- Tabs (square or ramped 3D), lead-in and lead-out, ramping, overcut, climb or conventional cutting, and stock left for a finishing pass.
- Arcs are written as G2/G3 where cuts follow circles, which gives smoother curves and shorter files.
- G-code can be written for GRBL and several other common controller types, in millimetres or inches, as one file or one file per tool.

### 3D carving
Import a greyscale picture (PNG or JPG, white is high) or a 3D model (STL) as a relief, then clear it with a 3D rough pass and smooth it with a 3D finish pass.

### Tools
- A tool library for flat end mills, ball noses, V-bits, drills, drag knives and thread mills.
- **Add from list:** pick the bits you own from about 200 common metric and imperial bits.
- **Feeds and speeds calculator:** choose the material and how stiff your machine is, and CncCraft suggests spindle speed, feed, plunge, depth per pass and stepover.

### Preview and checks
- A cut preview that removes material as the job plays, viewed from the top, in a shaded 3D view, or as front and side sections. Every view pans and zooms, and the preview can be maximized to fill the window. The 3D view has soft lighting, wood grain and a contact shadow; flat-ended pocket and through-profile cuts are drawn from the exact shape outlines, with pocket corners rounded by the bit radius, giving smooth walls and the floor corners the bit actually leaves (a through profile shows the finished part; use Top/Front/Side for the stock and waste), while V-carve and ball-nose cuts keep their shaped edges.
- **Checks before cutting** catch rapid moves into material, cuts outside the stock or deeper than the bit's flutes, parts that will come loose, clamps in the way, and jobs bigger than the machine.
- A warning about shapes that are not closed points at the exact open end, gap or unjoined corner: **double-click the warning** (or the status bar) to zoom in, then press **Fix: Join…** or **Fix: Close…** to repair it, and double-click again to step through any remaining problems.

### Machine control
- Connect over USB serial, or try everything with the **built-in simulator**.
- Jog, home, unlock, set zero, probe Z with a touch plate, and control the spindle.
- Start, pause, stop, and frame the job, with a progress bar.
- **Resume from…** restarts a job part-way through after a broken bit or a power cut.
- Save your machines with all their settings, and switch between them.
- A console for sending your own commands.

---

## Getting started

### Download
Download the latest `cnccraft.exe` from the Releases page and run it. Nothing needs installing.

### Build from source
You need [Rust](https://www.rust-lang.org/tools/install) (the stable toolchain).

```bash
git clone https://github.com/DnG-Crafts/CnC-Craft
cd cnccraft
cargo build --release
```

The program is built as `target/release/cnccraft.exe` (on Windows the icon and version details are built in). Run the tests with `cargo test --release`.
On Linux, run `./target/release/cnccraft --install-desktop` once: it installs a menu entry and the icon into `~/.local/share`, which Wayland panels and task bars use to show the CncCraft icon instead of the generic Wayland icon.

### Your first job, in a few steps
1. **Setup** (right-hand ribbon): enter the stock size and thickness, and where X/Y and Z zero are.
2. Draw a shape, for example a rectangle with **R**, then type its exact size in the number bar.
3. Select it, open **Layers** and click **Pocket** (or **Profile** to cut it out).
4. Set the depth and pick the tool. Speeds come from the tool library.
5. Choose **Toolpaths → Preview cut…** to watch it cut.
6. Choose **Toolpaths → Check job before cutting…**, then **File → Save G-code…**, or connect on the **Machine** tab and press **Start**.

The [wiki](../../wiki)  explains every tool and setting step by step.

---

## Files
| | |
|---|---|
| `.ccraft` | CncCraft project (design, toolpaths, stock and tools) |
| `.svg`, `.dxf` | Import and export designs |
| `.png`, `.jpg`, `.stl` | Import 3D reliefs |
| `.nc`, `.gcode`, `.tap`, `.ngc` | G-code output |
| `tools.json` | Saved tool library |

Settings are stored in `%APPDATA%\CncCraft\settings.json`.

---

## Safety
CNC machines are dangerous. Always check G-code in the preview before you run it, keep clamps clear of the cutting area, wear eye and hearing protection, and stay with the machine while it runs. Keep a hand near the emergency stop. The **STOP** button in CncCraft is not a replacement for a hardware emergency stop.

## License

CnC Craft is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. See [LICENSE](LICENSE).

CnC Craft is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
