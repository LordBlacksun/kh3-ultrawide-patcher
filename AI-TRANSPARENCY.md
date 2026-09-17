# AI Transparency Notice

In the spirit of honesty — especially for a tool that edits a game executable — here is how
this project was built.

## How it was made

KH3 Ultrawide Patcher was **fully implemented with AI assistance, under human direction** — the
code was written by an **AI coding agent** (Anthropic's Claude, via Claude Code) following the
maintainer's specifications, review, and testing. In practice that means:

- **Effectively all of the code** — the **Rust core** (detection, hashing, backup, patching,
  verification), the **Svelte UI**, and the **test suite** — was AI-generated from the
  maintainer's specifications and refined iteratively.
- The **fix itself** — the byte signatures, the aspect edits and the Hor+ projection fix — was
  reverse-engineered and validated on real hardware, and every release is **human-tested
  in-game** before it ships.
- The maintainer directed the work, made the design decisions, reviewed the output, and is
  responsible for what ships.

## Why we're telling you

You're trusting this tool to modify your game's executable, so you deserve to know how it was
built and to be able to verify it yourself. AI-written code can be wrong in subtle ways, so the
project leans on safeguards that don't depend on trust:

- **golden tests** proving the patcher reproduces the known-good patched executable
  **byte-for-byte** (from a clean copy and from older patched builds), and that revert restores
  the original;
- **in-game measurement** of the Hor+ projection fix: the rendered projection matrices were read
  live (read-only) during play to confirm every camera renders at its 16:9 vertical angle;
- an **adversarial, multi-reviewer code audit** before the first public release;
- an **offline-by-design** app — it makes no network calls and grants itself no network
  permission — with a deny-oriented `.gitignore` so no game data or build-machine information
  is ever committed;
- and it is **open source (GPL-3.0)** — read every line, build it yourself, compare the hashes.

## Not a substitute for your own judgment

Please review the code if you have any doubt. Issues and pull requests that find problems —
AI-introduced or otherwise — are genuinely welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md).
