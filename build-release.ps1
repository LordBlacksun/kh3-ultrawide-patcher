#!/usr/bin/env pwsh
# Build a release with build-machine paths stripped from the binary.
#
# Rust embeds absolute paths from the build machine (e.g. C:\Users\<you>\.cargo\... and the
# workspace path) into the executable, inside dependency panic-location strings. On a public
# release that would leak your Windows username. This wrapper remaps those prefixes to neutral
# values at build time using rustc's --remap-path-prefix, so nothing machine-specific ships
# in the exe. Nothing is hard-coded: the real prefixes are read from the environment at runtime.
#
# (Cargo's stable `trim-paths` profile option would be the tidier fix, but it is not stabilized
#  in the toolchain this project targets, so we use --remap-path-prefix instead.)
#
# Usage:  ./build-release.ps1        # run from the repo root

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

# Unit-separator-delimited so path prefixes containing spaces are handled correctly
# (a plain space-separated RUSTFLAGS would split the workspace path).
$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS =
    "--remap-path-prefix=$($env:USERPROFILE)=C:\Users\user" + $sep +
    "--remap-path-prefix=$($PSScriptRoot)=."

Write-Host "Building release with path remap (username / install path stripped from binary)..."
npm run tauri build

Write-Host ""
Write-Host "Done. Artifacts under src-tauri\target\release\ (portable exe) and"
Write-Host "src-tauri\target\release\bundle\nsis\ (installer)."
Write-Host "Tip: confirm cleanliness with  Select-String -Path <exe> -Pattern 'Users\\\\<yourname>'  (expect no match)."
