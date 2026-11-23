# Pictures for the docs

Everything in [`docs/images`](../../docs/images) comes from here:

- **Screenshots** of the real After Effects and Illustrator panels (`dist/cep`) and the real
  Figma plugin UI (`figma-plugin/dist/ui.html`), with sample data. `cep-stub.js` stands in for
  CEP and the panels' background extensions, and `figma-stub.js` for the bridge, so no Adobe app,
  bridge or Figma is needed. The Figma screenshot of a send uses the plugin's own test fixture
  and the real core, so its warnings are the converter's.
- **Diagrams** in `diagrams/*.html`, each rendered twice (`-light.png`, `-dark.png`) with a
  transparent background. The docs pick one with `<picture>` and `prefers-color-scheme`.

```sh
node tools/build-panels.mjs                # the extensions, if dist/cep is missing or old
cd figma-plugin && npm ci && cd ..         # puppeteer-core comes from the plugin's packages
node tools/docs-images/render.mjs          # everything
node tools/docs-images/render.mjs overview # only outputs whose names contain "overview"
```

It needs Chrome or Edge (or `PULLEY_TEST_BROWSER`), runs it headless with a throwaway profile,
and renders at 2× so the pictures stay sharp on high-density screens.

To change a sample (the panel states, warnings, selection names), edit the scenarios at the top
of `cep-stub.js`. Warning texts there are copied from the code that writes them (`aeplan.rs`,
`apply.jsx`, `bg.js`); keep them in step if those change.
