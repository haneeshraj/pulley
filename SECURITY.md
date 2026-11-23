# Security

Pulley runs entirely on one computer. It has no servers, accounts, telemetry or internet access.
This document lists what it protects, what can attack it, and how each attack is handled. Items
marked *tested* are covered by the automated tests named in [docs/VERIFICATION.md](docs/VERIFICATION.md).

## What is protected

- Your documents: After Effects projects, Illustrator documents, Figma files. Pulley must never
  change a source document, never leave traces in its undo history, and add exactly one undo step
  to the After Effects project per import.
- After Effects and Illustrator themselves: content from a document must never become code that
  runs in them.
- The bridge's secrets: the session token (lets the Adobe extensions connect) and Figma pairing
  keys.

## Components and trust boundaries

| Component | Runs as | Talks to |
|---|---|---|
| Pulley bridge (Rust) | your user account | WebSocket server on `127.0.0.1` only (default port 47290) |
| After Effects and Illustrator extensions (CEP 12, Chromium) | inside the Adobe apps | the bridge; their app through fixed ExtendScript functions |
| Figma plugin main thread | Figma's plugin sandbox | the plugin UI only (no network) |
| Figma plugin UI (iframe, opaque origin) | Figma | the bridge only (`networkAccess` lists `ws://localhost:47290`; Figma rejects IP addresses there) |

Every scene is untrusted data, wherever it comes from: an Illustrator document, a Figma file, a
`.pulley` file, or a client of the bridge.

## Threats and defenses

### 1. A web page in your browser connects to the bridge

Any page can try to open `ws://127.0.0.1:47290`.

- **Loopback only.** The bridge binds `127.0.0.1`, never `0.0.0.0`; other machines can't reach it.
- **Host check (DNS rebinding).** The handshake is refused unless `Host` is `127.0.0.1:<port>` or
  `localhost:<port>` (*tested*).
- **Origin allowlist.** Only `file://` (the Adobe extensions) and `null` (Figma's sandboxed
  plugin iframe) are accepted; ordinary pages send their own origin and are refused before the
  WebSocket upgrade (*tested*).
- **Authentication.** `Origin: null` is also what any sandboxed iframe or `data:` page sends, so
  Origin alone proves nothing. Every connection must send `hello` with a valid credential within
  5 seconds or it is closed (*tested*):
  - Adobe extensions (Origin `file://`) present the **session token**: 256 random bits, created
    at every bridge start and stored in `<config>/session.token`, which only your user can read.
    A web page can't read files.
  - The Figma plugin (Origin `null`) presents its **pairing key** (256 bits). It gets one only by
    sending a **pairing code** that the bridge shows in its tray menu after you choose *Pair Figma
    plugin…*. A code is 8 characters from a 32-symbol alphabet, valid for 2 minutes, single use,
    and invalidated after 5 wrong attempts; each failure also costs 300 ms (*tested*). A page
    therefore has 5 guesses at a 40-bit code during a window you opened yourself. For
    convenience the code is also copied to the clipboard; a page can't read the clipboard
    without your permission, and a program running as you is out of scope (section 4).
  - Credentials are compared in constant time. A role must match its origin (an extension can't
    claim to be Figma and the reverse) (*tested*).
- **Browser protection.** Chrome 147+ also asks before a public site may reach local
  addresses (Local Network Access); Safari blocks it outright.

### 2. A crafted document or file (malformed or hostile scene)

- **Strict parsing.** Scenes and messages are parsed with unknown fields rejected and only finite
  numbers accepted. Protocol messages are parsed in two steps (envelope, then the exact message
  type) (*tested*).
- **Hard limits** on message size (2 MB per WebSocket frame, 64 MB per reassembled scene), item
  count, nesting depth (48), vertices, text length, stops, dashes, paints, names and coordinate
  range. They are listed in [schema/SCENE.md](schema/SCENE.md) and enforced in Rust
  (`pulley-core/src/limits.rs`) (*tested*).
- **Checked twice.** The bridge validates every scene natively before forwarding it, and the After
  Effects extension validates it again (WebAssembly) before building anything.
- **Fuzzing.** The scene, protocol, SVG path, Illustrator capture and Figma node parsers are
  fuzzed: a stable mutation harness runs in every `cargo test`, and libFuzzer targets run in CI.
  A long local run (500,000 inputs) found no crash; it found one quadratic slowdown (very long
  names in the Figma and Illustrator converters), now fixed. Details in
  [docs/VERIFICATION.md](docs/VERIFICATION.md#fuzzing).
- **Images** travel as whole files inside the scene. Only PNG, JPEG, GIF, BMP, TIFF and PSD are
  accepted, recognized by their signature, with the header's size checked against the scene
  (pixels themselves are never decoded by Pulley). Before the After Effects extension writes an
  image to disk it checks that the image's id is the SHA-256 of its bytes, because the id names
  the file. After Effects then reads the file with its own importers, as it would any image you
  import; a `.pulley` file from someone else can therefore make After Effects open an image file,
  like any shared project can.
- **No unsafe code** in Pulley's Rust (`forbid(unsafe_code)`; the WebAssembly glue generated by
  wasm-bindgen is the only exception and is generated code).

### 3. Code injection into After Effects or Illustrator

- **No source built from data.** ExtendScript code is never assembled from payload content. The
  extensions call fixed, pre-written functions; the only arguments are paths that Pulley generates
  itself and short flags, passed as escaped string literals (printable ASCII only, quotes and
  backslashes escaped), and function names are checked against a fixed pattern.
- **Plans are data.** After Effects receives a line-based plan in a temporary file. A fixed
  interpreter maps each line to a few DOM calls; strings in the plan are percent-encoded and
  decoded with the native `decodeURIComponent`, then assigned to properties (layer names, text).
  Nothing is evaluated. An image line names its file only by content hash and extension, checked
  against fixed patterns (64 hex digits; png, jpg, gif, bmp, tif, psd), in Pulley's own media
  folder.
- **No `eval`** anywhere, in JavaScript or ExtendScript. ExtendScript has no native JSON; Pulley's
  ExtendScript only *writes* JSON with a small serializer and never parses it.
- **Node.js is off** in the CEP extensions (the default); file access uses CEP's `cep.fs`.
- **No shell.** The only process Pulley ever starts is its own bridge, from its known install
  location, without a shell and without arguments.
- **Figma plugin UI:** everything received is shown with `textContent`; the only `innerHTML` is
  for icons compiled into the plugin at build time.

### 4. Other local processes and other users

- Files are written only to Pulley's own folders: `<config>` (`%APPDATA%\Pulley` or
  `~/Library/Application Support/Pulley`), its `logs`, `tmp` and `media` subfolders, and the
  extensions' temp files under the same folder. Temporary plans, SVG exports and gradient presets
  get random names and are deleted after use. Image files go to `media`, named by their SHA-256,
  and are **kept**: After Effects projects link to them. Pulley never deletes them; you can.
- The Illustrator extension reads the files of linked images in the selection (read only), as
  Illustrator itself does.
- Secrets are written atomically, with mode 0600 on macOS; on Windows `%APPDATA%` is private to
  your account.
- **Out of scope: malware running as your user.** It can read the session token and pairing keys,
  and it can drive After Effects directly anyway. Figma pairing keys are stored as-is in
  `settings.json` for this reason (hashing them would not stop a process that can read the token
  file). *Forget paired Figma plugins* in the tray revokes them all.

### 5. Resource exhaustion

- Frame and scene size caps, chunk order checks and a per-connection reassembly limit; open
  transfers expire after 15 minutes; idle connections close after 120 seconds without traffic
  (clients ping every 20 seconds); pairing attempts are rate-limited (*tested*).
- After Effects imports can be cancelled from the panel or from the sending app.
- A local process can still exhaust or kill the bridge; that is accepted (same user).

### 6. Your documents stay untouched

- **Illustrator:** the capture only reads the document. Text outlines, symbols, very large
  paths, effects (expanded) and embedded images (saved as PNGs in Pulley's temp folder, deleted
  once read) come from copies in a temporary document that is closed without saving; the
  document's selection is never changed (deselecting marks a document as changed: checked). *Tested* on
  13 fixture documents: the document is never marked as changed and the selection is kept.
- **Figma:** the plugin only reads node properties and image bytes; it never creates, changes or
  flattens nodes, so the file and its undo history are untouched.
- **After Effects:** each import is one undo group ("Pulley Import"). Gradient colors are applied
  through generated presets, which needs a brief selection of each gradient property; your layer
  selection is restored afterwards. *Tested:* one Undo removes a whole import, including a comp
  created for it, the gradient presets, and imported images with their "Pulley images" folder,
  and a layer you had selected is selected again afterwards.

### 7. Logs and privacy

- No telemetry or network access beyond `127.0.0.1`.
- Logs (7 daily files) record connections, transfers, errors and timings. Geometry and text
  content are logged only with *Debug logging* turned on.

### 8. Supply chain

- Dependencies are few and mainstream ([docs/DEPENDENCIES.md](docs/DEPENDENCIES.md)), locked
  (`Cargo.lock`, `package-lock.json`), and checked with `cargo deny` (licenses, crates.io as the
  only source, advisories), `cargo audit` and `npm audit`.
- The extensions and the plugin load no remote code and include no third-party JavaScript; the
  WebAssembly module is built from this repository and inlined.
- Known advisories in the lockfile affect only Linux-only GTK dependencies of the tray crates,
  which are never built for Windows or macOS (details in DEPENDENCIES.md).

## Reporting a problem

Please report security problems **privately**, not in a public issue: on GitHub, open the
repository's **Security** tab and choose **Report a vulnerability**. Include the Pulley version,
your operating system, the app versions involved, and steps or a file that shows the problem.
You'll get an answer as soon as possible; fixes are released before details are made public.

Ordinary bugs (crashes, wrong colors, missing layers) can go in the public issue tracker.
