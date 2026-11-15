# How Pulley reads an Illustrator selection

**Decision:** read the selection directly through Illustrator's scripting DOM, read-only. Copy
items into a temporary document and export SVG only for what the DOM can't give cheaply: text
outlines (optional), symbol artwork, and paths with more than 1,000 points.

All numbers: Windows 11, i9-13900H, Illustrator 29.5, scripts run from a CEP extension through
the test harness ([tests/harness](../tests/harness)).

## Milestone 1: the first measurement pointed the wrong way

The probe ([tests/probes/cep-probe](../tests/probes/cep-probe)) created 500 paths of 20 points by
script and then read them back:

| Read | Time |
|---|---|
| Every point's anchor, left and right direction (DOM) | **78.6 s** |
| Anchors only, second pass | 0.12 s |
| SVG export of a temporary document with the same paths (301 KB) | 0.63–1.4 s |

This suggested exporting SVG for all geometry. But the paths had been created by the same script
in the same session, and Illustrator is known to keep PathPoint objects alive for the rest of a
session; the slowdown could be an artifact of that.

## Milestone 4: measured on a document opened from disk

[`tests/probes/ai-bench.jsx`](../tests/probes/ai-bench.jsx) opens a saved fixture with 500
paths × 20 points (like a real user document) and times each operation:

| Operation | Items | Time | Per item |
|---|---|---|---|
| Read name, opacity, blend mode | 500 | 2 ms | — |
| Read `uuid` | 500 | 1 ms | — |
| Read fill color | 500 | 3 ms | — |
| **Read every point (anchor, left, right)** | 100 paths × 20 points | **31 ms** | 0.3 ms per path |
| Create a temporary document | 1 | 955 ms | — |
| Make the source document active again | 1 | 278 ms | — |
| Duplicate into the temporary document (in the background) | 100 | 7.7 s | 77 ms |
| Rename those duplicates | 100 | 6.2 s | 62 ms |
| Read name and opacity of those duplicates | 100 | 12.4 s | 124 ms |
| Duplicate into the temporary document (active) | 100 | 14.5 s | 145 ms |
| Select items in the source document | 100 | 3.3 s | 33 ms |
| Export the temporary document as SVG | 1 | 629 ms | — |
| Close the temporary document without saving | 1 | 389 ms | — |

Findings:

- Reading geometry from a real document is fast: 31 ms for 100 paths, against 78.6 s for paths
  created in the same session. The milestone 1 number was the artifact.
- Anything that touches a second document is slow: every operation on items in a background
  document costs tens of milliseconds. Copying a whole 500-item selection would take 40–70 s
  before any export.
- Selecting items by script is slow too (33 ms each) and would change the user's selection.
- The source document stayed unmodified (`saved` unchanged).

So the DOM is the fast path, and the temporary document is kept for the cases that need it.

## The capture as built

[`panels/illustrator/jsx/capture.jsx`](../panels/illustrator/jsx/capture.jsx) walks the
selection once and records everything pulley-core needs, including each path's points and
handles. Only these items are copied into a temporary document and exported to SVG:

- text, when *Include text outlines* is on (outlines are made from the copy; `createOutline()`
  on the original would replace it);
- symbol instances (their artwork is only reachable by breaking the link, which is done on the
  copy);
- paths with more than 1,000 points.

The temporary document is sized to the artboard, gets a reference square at the artboard corner
for alignment, and is closed without saving. The user's document is never exported, because
`exportFile` marks the exported document as saved.

### Results on the fixtures

Capture of each fixture document with everything selected (latest run, 2026-11-15); the
document was marked clean right before each capture to see whether the capture changes it:

| Fixture | Items | Time | Document still unmodified | Selection kept |
|---|---|---|---|---|
| clipping | 2 | 66 ms | yes | yes |
| colors-cmyk | 5 | 65 ms | yes | yes |
| compound | 3 | 65 ms | yes | yes |
| gradients | 5 | 65 ms | yes | yes |
| groups | 1 | 70 ms | yes | yes |
| hidden | 1 | 63 ms | yes | yes |
| shapes | 7 | 67 ms | yes | yes |
| strokes | 13 | 86 ms | yes | yes |
| transforms | 5 | 65 ms | yes | yes |
| large (one path, more than 1,000 points: temporary document) | 1 | 2.8 s | yes | yes |
| text (with outlines: temporary document) | 5 | 4.4 s | yes | yes |
| unsupported (includes a symbol: temporary document) | 4 | 2.3 s | yes | yes |
| perf-500x20 | 500 | 1.9 s | yes | yes |

The 500 × 20 capture repeated five times in the same session took 504, 465, 474, 570 and 445 ms:
no slowdown from repeated pushes in this run. (The first capture includes compiling the script
and warming up the DOM.)

Pulley's scene is then built from the capture by pulley-core in WebAssembly; the converted
fixtures are checked against golden scenes in
[`core/pulley-core/tests/illustrator_fixtures.rs`](../core/pulley-core/tests/illustrator_fixtures.rs).

## What would change the decision

- A document where reading points is slow again (the PathPoint retention reported for long
  sessions): the per-path cost would show up in the capture time. The capture reads each path
  once, which limits the damage.
- Text outline export dominates text-heavy pushes (4.4 s for 5 text items, including the
  temporary document). It is off by default and only matters when fonts may be missing in
  After Effects.
