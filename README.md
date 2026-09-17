<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" height="96" alt="KH3 Ultrawide Patcher icon" />

<h1>KH3 Ultrawide Patcher</h1>

<p><b>True 21:9 / 32:9 ultrawide for KINGDOM HEARTS III</b> — full-width, Hor+ on every camera, no stretch or zoom.</p>

<p>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License: GPL-3.0" /></a>
  <a href="https://github.com/LordBlacksun/kh3-ultrawide-patcher/releases/latest"><img src="https://img.shields.io/github/v/release/LordBlacksun/kh3-ultrawide-patcher?display_name=tag&cacheSeconds=3600" alt="Latest release" /></a>
  <a href="https://github.com/LordBlacksun/kh3-ultrawide-patcher/releases"><img src="https://img.shields.io/github/downloads/LordBlacksun/kh3-ultrawide-patcher/total?cacheSeconds=300" alt="Downloads" /></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078D6" alt="Platform: Windows" />
  <img src="https://img.shields.io/badge/built%20with-Tauri%20%2B%20Svelte-FFC131" alt="Built with Tauri + Svelte" />
</p>

</div>

A small, elegant desktop app that patches **KINGDOM HEARTS III** (Steam & Epic, Unreal
Engine 4) to render **true ultrawide** — full-width **21:9 / 32:9** with correct
proportions and **Hor+ on every camera** (exploring, combat, team attacks, in-engine
cutscenes) instead of pillarboxing a 16:9 image into black bars.

It finds your install on its own, backs up the executable, applies four aspect-ratio edits
plus a small Hor+ projection fix, verifies the result, and reverts in one click.

> Built with Tauri 2 + Svelte. No telemetry, no network access — everything runs locally.

---

## Screenshots

The whole flow is three steps — **Detect → Configure → Patch**. The Configure step lets you
choose your resolution three ways:

**Presets** — common 21:9 and 32:9 resolutions:

![Configure screen showing ultrawide resolution presets](screenshots/1.png)

**My display** — auto-detect your monitor:

![Configure screen auto-detecting the current display](screenshots/2.png)

**Custom** — type any width × height:

![Configure screen with a custom width and height](screenshots/3.png)

---

## Features

- **Auto-detects the game** on **Steam** (registry → `libraryfolders.vdf` →
  `appmanifest_2552450.acf` → exe) and **Epic Games** (launcher manifests), with a
  **manual browse** fallback.
- **Common ultrawide presets** — 2560×1080, 3440×1440, 3840×1600, 5120×2160 (21:9),
  3840×1080, 5120×1440 (32:9) — plus **auto-detect my display** and **custom W×H**.
- **Hor+ on every camera, any resolution:** aspect = `W/H`, and every camera's view is widened
  at render time so it keeps the vertical framing it has at 16:9.
- **Safe by construction:** automatic backup (to `%LOCALAPPDATA%`), SHA-256 verification,
  idempotent re-runs, and one-click **revert** to the exact original.
- **Updates older patches:** executables patched by v1.0.x are detected and upgraded in place.
- **16:9-aware:** picking a 16:9 resolution is a no-op (nothing is written).

---

## How it works — the fix

KH3 has no native ultrawide. Two things have to change, and the file size stays the same.

**1. Aspect ratio — four 4-byte float edits.** The game hard-codes 16:9 for its output and
its cameras:

| edit | what | before | after (example: 3440×1440) |
|---|---|---|---|
| output / render aspect (×1) | frame fills the screen | `AC 8B E3 3F` (1.7778) | `8E E3 18 40` (2.38889) |
| camera projection aspect (×3) | correct proportions, no stretch | `3B 8E E3 3F` (1.7778) | `8E E3 18 40` (2.38889) |

The famous single "`AC 8B E3 3F`" community edit is only the first of these, which is why it
*stretches*.

**2. Hor+ on every camera — the projection fix.** KH3's camera FOVs are *horizontal*, and they
come from game data: the regular camera uses 100°, special attacks 40–110°, and in-engine
cutscenes change FOV with every shot. On a wider screen each camera keeps its horizontal view
and loses height, which looks zoomed in — about 1.34× at 21:9 and 2× at 32:9. Instead of chasing
individual values, the patch fixes the one place every FOV passes through, Unreal's
`FMinimalViewInfo::CalculateProjectionMatrixGivenView`: right after it computes `tan(FOV/2)`,
the result is scaled by `aspect × 9/16`. Every camera then shows exactly the vertical framing its
FOV has at 16:9, with the extra width at the sides. (At 16:9 the factor is 1 — no change.)

In bytes: the instruction after each of the function's two `call tanf` sites is replaced by a
call to a tiny leaf routine that multiplies the tangent and re-executes the instruction it
displaced. The two routines and their constant (51 bytes) go into unused int3 padding in the
executable. Every site is located by byte signature; if anything doesn't match exactly once, the
patch aborts without writing.

v1.0.x widened three camera FOV constants instead, which only reached cameras that use the
engine's default FOV. v1.1.0 restores those values when it updates an older patch, so no camera
is widened twice.

---

## Install

### Option A — download (recommended)
Grab the portable `.exe` from the **Releases** page, run it, and
follow the three steps: **Detect → Configure → Patch**.

### Option B — build from source
Prerequisites: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20 or 22 (LTS), and
the WebView2 runtime (preinstalled on Windows 11).

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce an installer + portable exe under src-tauri/target/release
```

---

## Verifying the download

The released `.exe` is **unsigned** (see the SmartScreen note under *Notes & known issues*), so
verify it however reassures you:

1. **Checksum** — compare your download's SHA-256 against the value published on the **Releases**
   page:
   ```powershell
   Get-FileHash .\kh3-ultrawide-patcher.exe -Algorithm SHA256
   ```
2. **VirusTotal** — each release links a VirusTotal scan; you can also upload the file yourself.
3. **Build it yourself** — clone the repo and run `./build-release.ps1` (the *Build from source*
   option above). The app makes **no network calls** and grants itself **no network permission**,
   so a from-source build does exactly what the released binary does — nothing phones home.

---

## Using it

1. **Detect** — the app locates your KH3 install and shows the executable, its state
   (clean / already patched / older patch to update / unknown build), and whether a backup exists.
2. **Configure** — pick a resolution preset (or your display / a custom size).
3. **Patch** — the original exe is backed up, the edits are applied, and the result is
   verified. In game, set **Borderless Fullscreen** at your chosen resolution. (With the game's
   HDR setting on, KH3 only offers **Fullscreen** — that works just as well.)

**Revert** at any time restores the exact original executable from the backup.

### After a game update
Steam/Epic updates and *“Verify integrity of game files”* restore the original exe (and
thus undo the patch). Just run the patcher again — it’s idempotent and re-applies cleanly.

---

## Notes & known issues

- **Administrator:** if the game is installed under `Program Files`, Windows may require
  the patcher to run **as administrator** to write the exe. If a write is denied, relaunch
  it elevated (right-click → *Run as administrator*).
- **SmartScreen / antivirus:** the app is an unsigned executable that edits another
  executable, which can trip Windows SmartScreen or AV heuristics. It is open source and
  makes no network connections; you can build it yourself and compare hashes. Code signing —
  via the free **SignPath Foundation** program for open-source projects, which signs under the
  Foundation's name (so it doesn't expose the maintainer) — is planned. (If SmartScreen appears:
  *More info → Run anyway*.)
- **Epic Games:** Epic detection is implemented to the documented launcher-manifest format
  but was developed and verified on the Steam build — **a community tester on Epic is very
  welcome** (please open an issue with results).
- **Pre-rendered FMV cutscenes & boot/logo videos stay 16:9 (pillarboxed) — this is expected,
  not a bug.** They're native pre-rendered video files, not real-time rendering, so the patch
  can't widen them; stretching them would distort faces and logos. In-engine (real-time)
  cutscenes *do* render full ultrawide.
- **Upgrading from v1.0.x:** open v1.1.0 and patch again. It recognises the older patch, swaps
  its three FOV edits for the projection fix, and keeps your aspect edits.
- **v1.0.0's “Also widen combat & team-attack cameras” option was removed.** Checked against the
  engine's own property data, its edits turned out not to be cameras at all: with the box
  ticked, v1.0.0 changed an ocean wave's wind angle and the engine's default scene-capture FOV.
  Patching again with v1.1.0 restores both values (**Revert** removes them too). The projection
  fix now covers the team-attack cameras that option was meant for.

---

## Safety & verification

- KH3 is single-player with **no anti-cheat**, so editing the exe is safe online-wise.
- The patch **always backs up** the original first and can restore it byte-for-byte.
- The byte edits are derived from, and unit-tested against, the known build:
  - clean baseline SHA-256 `F53C398936560D543F2AA8E6283733572FDF8AD7C14E03459C12E039CB1BD0BC`
  - patched 3440×1440 (v1.1.0, Hor+ on every camera) SHA-256 `9A2582F62C1E0142AA1B416DD3D9D403D32CC2EB6DA462FCEA5F4E2163F5C96C`
- Golden tests confirm the patch engine reproduces that build **byte-for-byte** from a clean
  baseline, that executables patched by v1.0.x upgrade to exactly the same bytes, and that
  revert restores the baseline.
- The projection fix was verified in-game by reading the rendered projection matrices live
  (read-only) through a full boss fight: the regular 100° camera, the locked-on camera, special
  attacks and every cutscene shot rendered at exactly their 16:9 vertical angle.
- On an unrecognised build the app falls back to signature search; it never guesses — a
  signature that matches more than once, or code that doesn't look exactly as expected, aborts
  the operation.

---

## Tech

Tauri 2 (Rust core) + SvelteKit (Svelte 5). The Rust side does all file I/O, detection,
hashing, backup, patching, and verification; the web UI is purely presentational. The app
makes **no network calls** and grants itself **no network permission** — its Tauri capability
set is just window controls and the file-open dialog (`core:default` + `dialog:allow-open`).
(Tauri's core pulls an HTTP client in transitively, but this app never configures or invokes
it; you can confirm with `cargo tree` and the capability file.)

---

## Contributing

Contributions are welcome — especially **testing on Epic Games** and reporting **new game
builds**. See [CONTRIBUTING.md](./CONTRIBUTING.md).

## Transparency

This project was **fully implemented with AI assistance, under human direction** (with in-game
testing on real hardware). See the [AI Transparency Notice](./AI-TRANSPARENCY.md).

## License

[GPL-3.0-only](./LICENSE).

---

## Disclaimer

This is an unofficial, fan-made tool, not affiliated with or endorsed by Square Enix, Disney,
Epic Games, or Valve. *KINGDOM HEARTS* is a trademark of its respective owners. It modifies
your own copy of the game executable, always backs it up first, and can revert byte-for-byte —
but you use it **at your own risk**. Full text: [DISCLAIMER.md](./DISCLAIMER.md).
