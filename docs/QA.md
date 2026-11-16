# Release QA checklist

Manual checks before a release, on top of the automated tests (README → Tests). Run them on
**Windows and macOS**, with **After Effects and Illustrator 2025 and 2026**, from the **signed**
packages and installers (not from `dist/` with PlayerDebugMode). Note the version of every app.

Mark each item ✅ / ❌ and file what fails.

## Install

- [ ] Bridge installer runs on a clean user account; the bridge starts; the tray/menu bar icon
      appears (light and dark taskbar / menu bar).
- [ ] Uninstall removes the app; reinstalling over an existing install works.
- [ ] *Launch at login* on → log out and in → the bridge is running. Off → it isn't.
- [ ] Signed ZXPs install (Adobe UPIA or your installer of choice) **without** PlayerDebugMode;
      the panels appear under Window → Extensions → Pulley in both apps.
- [ ] macOS: the bridge app opens without a Gatekeeper warning (signed and notarized).
- [ ] Figma desktop: import the plugin; it opens and shows *Not paired*.

## Connection

- [ ] Both panels show *Connected* within a few seconds of the bridge starting; the AE panel
      lists Illustrator and Figma correctly.
- [ ] Quit the bridge: panels show *Not connected* and recover by themselves when it restarts
      (and the AE panel can start an installed bridge).
- [ ] Pair Figma: tray → *Pair Figma plugin…* → Ctrl+V in the plugin → *Connected* without
      clicking Pair. *Copy pairing code* copies it again; typing the code by hand also works.
      A wrong code shows an error; 5 wrong codes invalidate it; the code expires after 2 minutes.
- [ ] Close and reopen the Figma plugin: it connects without a new code.
- [ ] Tray → *Forget paired Figma plugins*: the plugin returns to *Not paired*.
- [ ] Figma in Chrome/Edge: the local network permission prompt appears; after allowing, it
      connects. In Safari it can't connect and the help text points to *Save .pulley file*.
- [ ] Two After Effects instances: the push goes to the one used last.

## Illustrator → After Effects

Use real documents, not only the fixtures: icons, illustrations with gradients, a logo with
text, a layout with clipping masks, a CMYK document, a Large Canvas document, a 500+ path file.

- [ ] Push with a comp open: layers land in that comp, centered (or at original positions with
      the option), with correct stacking order and names.
- [ ] Push with no comp open: a new comp the artboard's size is created and opened.
- [ ] Pull selection from After Effects with the Illustrator panel closed.
- [ ] Compare side by side: shapes, compound paths and holes (even-odd), strokes (width, caps,
      joins, miter, dashes), gradients (colors, direction, radial center and highlight), opacity
      and blend modes, clipping (matte and mask modes), groups.
- [ ] Text: fonts, sizes, colors per run, tracking, leading, alignment, area text box size;
      a missing font (uninstall one): outlines are used when *Include text outlines* is on,
      otherwise the panel lists the font.
- [ ] Unsupported items (image, mesh, blend, graph, symbol): each gives one clear warning; the
      rest imports.
- [ ] The Illustrator document is not marked as modified (no `*` in the title); Undo in
      Illustrator does nothing Pulley-related; the selection is unchanged.
- [ ] Layer modes: per item, merge, precompose.
- [ ] Cancel during a large push (500+ paths): the import stops and nothing is left behind.

## Figma → After Effects

- [ ] Send a frame with rectangles (individual corner radii), ellipses and arcs, polygons, stars,
      lines, vectors, boolean operations, text with mixed styles, linear and radial gradients,
      masks, clipped frames, nested auto-layout frames, component instances.
- [ ] Drop shadows and layer blurs (on a rectangle, a frame with text, a clipped frame, a text
      layer): After Effects shows "Pulley Drop Shadow" / "Pulley Blur" effects that look like
      Figma's (compare side by side at 100%).
- [ ] Inner shadows, background blurs, image fills, angular/diamond gradients: warnings shown,
      rest imports.
- [ ] Change a shadow in Figma and send again: the same effect updates; remove it and send: the
      effect is removed; an effect you added yourself in After Effects stays.
- [ ] Pull selection from After Effects while the plugin is open.
- [ ] Save .pulley file (desktop and browser) → Import .pulley file in After Effects gives the
      same result as a direct send.
- [ ] The Figma file shows no changes and nothing in its undo history.
- [ ] Display P3 file: the warning appears.

## Re-push

- [ ] Push a selection, push it again unchanged: nothing is added; the result says "updated".
- [ ] Change colors, move things, edit text in the source and push: the same layers change.
- [ ] Animate a layer's position (and, separately, a shape's path) in After Effects, then push a
      change: the animation stays and the result lists what was kept.
- [ ] Rename a layer and add an effect, then push: the name and effect stay.
- [ ] Delete an item in the source and push: its layer stays and is listed. Add an item: it
      appears as a new layer.
- [ ] Turn off *Update layers from earlier pushes* and push: copies are added.
- [ ] Ctrl+Z after a re-push restores the previous state.

## After Effects behavior

- [ ] One Edit → Undo removes the whole import (including a new comp); Redo restores it.
- [ ] Gradient colors are right; the layer selection you had before the import is restored.
- [ ] Panel closed: imports still work (background extension).
- [ ] Panel shows timings, warnings and missing fonts; the log button shows the bridge log.
- [ ] Light and dark UI brightness: panels and icons readable; switching updates live.
- [ ] Time a 500 × 20 import (per item and merge) and compare with the numbers in the README.

## Robustness

- [ ] Kill After Effects during an import, restart: Pulley reconnects; temp files are cleaned up
      on the next import.
- [ ] Sleep and wake the computer: connections recover.
- [ ] Open a damaged `.pulley` file (edit one by hand): a clear error, nothing imported.
- [ ] Bridge logs rotate daily and keep 7 files; nothing sensitive (geometry, text) at the
      default level.

## Icons (after the final icons arrive)

- [ ] Panel icons in both apps, light and dark; extension icons where the host shows them.
- [ ] Tray at 100%, 150%, 200% scaling (Windows) and the menu bar in light/dark (macOS); the
      transferring and error states.
- [ ] Installer header/sidebar (Windows), DMG background and volume icon (macOS), app icon in
      Explorer/Finder, Start menu/Launchpad.
- [ ] Figma plugin UI in light and dark themes.
