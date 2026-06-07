# Contributing to KH3 Ultrawide Patcher

Thanks for your interest! This is a small, focused tool, but contributions — bug reports,
testing on hardware and stores the maintainer can't reach, code, and docs — are very welcome.

## Ways to help

- **Test on Epic Games.** Detection is written to Epic's documented launcher-manifest format
  but was developed and verified on Steam. A real Epic run — success *or* failure — is the
  single most useful thing you can report. Please open an issue with the result.
- **Report a new game build.** If a Steam/Epic update changes the executable, the byte
  signatures may shift. Open an issue with the new build number and the state the app reports.
- **New resolutions / aspect ratios.** The math is general (`aspect = W/H`,
  `Hor+ FOV = 2·atan((W/H)·9/16)`), but real-world confirmation on 32:9, 48:9, etc. is helpful.
- **Code & docs.** Bug fixes, clarity, accessibility, and packaging improvements.

## Development setup

Prerequisites: [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org) 20 or 22 (LTS),
and the WebView2 runtime (preinstalled on Windows 11).

```bash
npm install
npm run tauri dev      # run the app in development
npm run tauri build    # produce an installer + portable exe
```

## Before you open a PR

- **Keep the tests green.**
  - Rust: `cargo test --manifest-path src-tauri/Cargo.toml`
  - Frontend: `npm run check` (svelte-check — must be 0 errors / 0 warnings)
- **The patch byte-table is safety-critical.** Any change touching `src-tauri/src/model.rs`
  or `src-tauri/src/patch.rs` must keep the **golden test byte-for-byte**. The golden test
  rebuilds the known patched executable from a clean baseline; run it against your own
  legally-owned copy (PowerShell):
  ```powershell
  $env:KH3_EXE_COPY = "C:\path\to\a\clean\KINGDOM HEARTS III.exe"   # a COPY you own — never commit it
  cargo test --manifest-path src-tauri/Cargo.toml
  ```
- **One logical change per PR.** For anything large, open an issue first so we can agree on the
  approach before you invest time.

## Two hard rules

1. **Never commit game data.** No game executable, no `Content/`, no copyrighted assets — not
   even in a test fixture or a commit you intend to amend later. The `.gitignore` is
   deny-oriented; keep it that way. Use your own legally-owned copy of the game for testing.
2. **Keep releases identity-clean.** Rust bakes the build machine's absolute paths (e.g.
   `C:\Users\<you>\.cargo\…`) into the binary via dependency panic-location strings. Build
   release artifacts with the path remap so your username doesn't ship in the exe:
   ```powershell
   ./build-release.ps1     # remaps build paths, then runs tauri build
   ```
   See that script for the exact `--remap-path-prefix` recipe if you'd rather set it yourself.

Also: keep the app offline — it makes no network calls and grants itself no network permission
(its Tauri capability set is just window controls + the file-open dialog). Please don't add
features or permissions that change that.

## Style

Match the surrounding code. The Rust side is plain, dependency-light, and does all the real
work (I/O, detection, hashing, backup, patching, verification); the Svelte UI is purely
presentational. Comments explain *why*, not *what*.

## Conduct

Be kind and constructive. Assume good faith. Harassment of any kind isn't welcome here.

## License

By contributing, you agree that your contributions are licensed under the project's
[GPL-3.0-only](./LICENSE) license.
