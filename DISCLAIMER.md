# Disclaimer

**KH3 Ultrawide Patcher is an unofficial, fan-made tool.** It is not affiliated with,
endorsed by, sponsored by, or associated with Square Enix, The Walt Disney Company, Epic
Games, or Valve. *KINGDOM HEARTS* and all related names are trademarks of their respective
owners and are used here only for identification.

## What the tool does to your files

- It modifies **your own, legally-owned copy** of the game executable, on your machine.
- It **always backs up** the original executable before writing, verifies the result with a
  SHA-256 hash, and can **revert** to the exact original byte-for-byte.
- It changes seven 4-byte values; the file size and all other bytes are unchanged.
- It makes **no network connections** and collects **no data**.

## No warranty

This program is distributed in the hope that it will be useful, **but WITHOUT ANY WARRANTY** —
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
the [GNU General Public License v3.0](./LICENSE) (sections 15–16) for the full text. **You use
it at your own risk.**

## Your responsibility

- Keep the backup the patcher creates. You can also re-acquire a clean executable at any time
  through your store's *Verify integrity of game files*.
- Modifying game files may be contrary to a publisher's or storefront's End-User License
  Agreement. KINGDOM HEARTS III is **single-player with no anti-cheat**, so there is no online
  or multiplayer component involved — but whether to modify your installation is your decision.
- Store updates and file-integrity checks restore the original executable, undoing the patch.
  This is expected; simply run the patcher again.

## SmartScreen / antivirus

Because this is an **unsigned** executable that **edits another executable**, Windows SmartScreen
or antivirus heuristics may flag it. The project is fully open source and makes no network
calls — you can read the code, build it yourself, and compare hashes. Code signing — via the
free **SignPath Foundation** program for open-source projects, which signs under the Foundation's
name — is planned.

## Limitation of liability

To the maximum extent permitted by applicable law, the authors and contributors shall **not be
liable** for any damages, data loss, or other harm of any kind arising from the use of, or
inability to use, this software.
