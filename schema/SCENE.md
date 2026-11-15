# pulley-scene v1

**Status: v1, implemented.** The Rust types in
[`core/pulley-core/src/scene.rs`](../core/pulley-core/src/scene.rs) and the checks in
[`validate.rs`](../core/pulley-core/src/validate.rs) are authoritative; the
[JSON Schema](pulley-scene-v1.schema.json) mirrors them for other tools. The tests parse and
validate every file in [`examples/`](examples/) with the Rust types; there is no automated check
of the examples against the JSON Schema file.

pulley-scene is the only data format that crosses between Pulley components (Illustrator panel,
Figma plugin, bridge, AE panel). Each component may use private encodings internally, for example
between a CEP panel's JavaScript and its own ExtendScript. Those never leave the component and are
produced and consumed by pulley-core. See [ARCHITECTURE.md](../docs/ARCHITECTURE.md).

## Conventions

| Topic | Rule |
|---|---|
| Coordinate space | "Scene space": pixels, **y-down**, origin at the top-left of `bounds`' artboard/frame. pulley-core converts Illustrator's y-up points and Figma's local transforms into scene space before a scene is built. |
| Units | `units.source` records the source unit; `units.pxPerSourceUnit` is the scale already applied (default 1, so 1 pt = 1 px). |
| Paths | AE's model: absolute `vertices`; `inTangents`/`outTangents` are **relative** to their vertex. The three arrays have equal length. Open paths ignore the first in-tangent and the last out-tangent. |
| Transforms | Geometry is baked into scene space. Only text keeps a `matrix`, because AE text can't be baked. |
| Colors | Gamma-encoded floats 0..1 in `colorSpace` (`srgb`, or `display-p3` for Figma P3 files). CMYK, spot and global swatches are resolved to RGB at the source using the source app's own conversion. |
| Opacity | Item `opacity`, paint `opacity` and gradient stop `opacity` are separate and multiply. |
| Order | Arrays of items are ordered topmost first, the same as AE's layer stack. |
| Text indexes | `start`/`end` are UTF-16 code-unit offsets into `content` (what Illustrator, Figma and AE scripting all use). Paragraphs are separated by `\r`. |
| Strings | `name`, `content` and warning text are **untrusted**. They are only ever passed to hosts as data and never evaluated. |
| Unknown fields | Rejected everywhere (`additionalProperties: false` / `#[serde(deny_unknown_fields)]`). |
| Defaults | Optional fields have a documented default (see the schema) and may be omitted. |

## Item types

- **group**: children plus optional `clip` (a clipping mask). Illustrator clip groups and Figma
  frames with `clipsContent` both become a group with `clip`. The clip path's own appearance is
  dropped, with a warning if it had any.
- **shape**: one or more paths sharing a `fillRule`, a list of `fills` and a list of `strokes`
  (bottom to top, as in Illustrator's appearance and Figma's paint lists). One path is a simple
  path; several form a compound path. Booleans in Figma are exported as their resolved geometry.
- **text**: `content`, style `runs`, `justification`, `kind` (`point`/`area`, plus `box` for area
  text), a `matrix`, and `outlines`: fallback shapes used when AE lacks the font.

Paints are `solid`, `linearGradient` (absolute `start`/`end`) and `radialGradient` (`center`,
`radiusPoint` on the 100% ring, optional `focalPoint`). Gradient stops carry `offset`, `color`,
`opacity` and `midpoint`. pulley-core splits them into AE's separate color and opacity stop lists.

Strokes carry `paint`, `width`, `cap`, `join`, `miterLimit`, `dashes` (dash, gap, dash, gap...),
`dashOffset` and `alignment`. AE shape strokes are always centered (checked in AE 25.5: there is no
alignment property), so `inside`/`outside` are emulated at apply time and reported with a warning.

## Versioning

- `schema` is always `"pulley-scene"`; `version` is an integer.
- Any change to this schema, including an added optional field, bumps `version`. With
  deny_unknown_fields there are no compatible minor versions, and that is intentional.
- When a reader gets a version it doesn't support, the error names the component to update:
  - scene newer than the reader: "This scene was made by a newer Pulley (pulley-scene v2). Update
    the Pulley After Effects panel to 0.x or later."
  - scene older than the reader supports: "Update the Pulley Illustrator panel" (or Figma plugin;
    taken from `source.app`).
- The bridge checks the version before forwarding, so a mismatch is reported to both sides.

## Limits

Enforced by pulley-core when parsing, before any other work. The ones JSON Schema can express are
also in the schema file.

| Limit | Value | Why |
|---|---|---|
| Scene message size | 64 MiB (after reassembling chunks) | 500 × 20 paths is about 0.45 MB; this leaves 100× headroom while bounding memory. |
| Nesting depth (groups) | 48 | Stops stack exhaustion from deeply nested input. Illustrator/Figma files rarely exceed 15. |
| Items in a scene (all levels) | 100,000 | Bounds AE apply time. |
| Paths per shape / clip | 10,000 | |
| Vertices per path | 100,000 | |
| Vertices in a scene | 2,000,000 | |
| Coordinate magnitude | ±10,000,000 px | AE comps max out at 30,000 px. Anything larger is corrupt or hostile. |
| `name` length | 1,024 UTF-16 units | |
| Text `content` length | 65,536 | |
| Text runs | 65,536 and each `start < end <= content length`, runs sorted and non-overlapping | |
| Gradient stops | 2..256 | |
| Dash array | 16 entries | AE supports 3 dash/gap pairs. Extra entries are reduced with a warning. |
| Warnings | 10,000 | |
| Numbers | finite only. JSON can't carry NaN/Infinity; serde_json rejects `NaN` tokens and overflow such as `1e400` (tested in `validate.rs`). | |
| Paints per shape | 32 | |

## Warning codes

Readers accept codes they don't know and show `message`. Each warning names the item it
concerns. Codes produced by the converters (in the scene):

| Code | Meaning |
|---|---|
| `unsupported_raster` | Placed or embedded image skipped (Illustrator) |
| `unsupported_gradient_mesh` | Gradient mesh skipped |
| `unsupported_graph`, `unsupported_plugin_item`, `unsupported_legacy_text`, `unsupported_item` | Graphs, blends/envelopes, legacy text, other objects skipped |
| `unsupported_pattern`, `unsupported_color` | Pattern or unknown color dropped |
| `unsupported_paint_angular`, `unsupported_paint_diamond`, `unsupported_paint_image`, `unsupported_paint_video`, `unsupported_paint` | Figma paints After Effects can't draw |
| `unsupported_effect`, `unsupported_node` | Figma effects and node types not imported |
| `appearance_not_imported` | The item looks larger than its path (art brush, live effect, extra stroke); only the basic fill and stroke are imported |
| `gradient_transform_approximated`, `radial_highlight_approximated`, `gradient_geometry_missing` | Gradient geometry simplified or unreadable |
| `clip_appearance_dropped`, `clip_missing`, `mask_unsupported` | Clipping path appearance dropped, or a clip/mask that couldn't be read |
| `corner_radius_dropped`, `stroke_cap_simplified` | Rounded polygon/star corners, decorative caps |
| `symbol_expanded` | Symbol instance imported as plain artwork |
| `text_on_path_outlined`, `text_area_transform_ignored`, `text_gradient_simplified` | Text simplifications |
| `geometry_missing`, `copy_failed`, `too_deep` | Something couldn't be read; the item or part of it is skipped |
| `display_p3_colors` | The Figma file uses Display P3 (scene-level) |

Codes added when the After Effects plan is built (shown the same way):
`stroke_alignment_emulated`, `dash_pattern_truncated`, `group_split`, `clip_matte_used`,
`text_gradient_simplified`, `text_skew_ignored`.

## Examples

- [`examples/basic-shapes.json`](examples/basic-shapes.json): a rectangle with a stroke, and a
  group with opacity, blend mode and a circle.
- [`examples/gradients-compound.json`](examples/gradients-compound.json): an even-odd compound
  path, a linear gradient fill, a radial gradient stroke with a focal point, dashes and inside
  alignment.
- [`examples/clip-and-text.json`](examples/clip-and-text.json): a clip group containing point text
  with two style runs, plus warnings.
