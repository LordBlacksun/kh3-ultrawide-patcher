//! Auto-detect the KINGDOM HEARTS III install on Steam or Epic, with a manual
//! browse fallback. No hardcoded install paths.

use crate::model::*;
use crate::{patch, vdf};
use std::path::{Path, PathBuf};

pub fn detect_all() -> DetectResult {
    let mut notes: Vec<String> = Vec::new();
    let mut candidates: Vec<GameInfo> = Vec::new();
    let backup_root = patch::backup_root_dir();

    let roots = steam_roots();
    if let Some(gi) = detect_steam(&roots, &backup_root, &mut notes) {
        candidates.push(gi);
    }
    if let Some(gi) = detect_epic(&backup_root, &mut notes) {
        if !candidates
            .iter()
            .any(|c| c.exe_path.eq_ignore_ascii_case(&gi.exe_path))
        {
            candidates.push(gi);
        }
    }

    if candidates.is_empty() {
        notes.push(
            "Couldn't auto-detect KINGDOM HEARTS III. Click \"Browse\" and select the game's exe.".to_string(),
        );
    }

    DetectResult {
        candidates,
        steam_root: roots.first().map(|p| p.to_string_lossy().to_string()),
        notes,
    }
}

/// Rank an install for auto-selection when several are found: prefer a clean baseline,
/// then a patchable build, then an already-patched one, then an unknown build.
fn state_rank(s: ExeState) -> u8 {
    match s {
        ExeState::CleanBaseline => 0,
        ExeState::Patchable => 1,
        ExeState::AlreadyPatched => 2,
    }
}

fn pick_best(mut found: Vec<GameInfo>) -> Option<GameInfo> {
    found.sort_by_key(|gi| (state_rank(gi.state), if gi.size == BASELINE_SIZE { 0 } else { 1 }));
    found.into_iter().next()
}

// ---------------------------------------------------------------------------
// Steam
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn steam_roots() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut cands: Vec<PathBuf> = Vec::new();

    // Per-user install (the common case).
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") {
        if let Ok(p) = key.get_value::<String, _>("SteamPath") {
            cands.push(PathBuf::from(p.replace('/', "\\")));
        }
    }

    // Machine-wide install path (covers a fresh/elevated/corrupted HKCU on a non-default drive).
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for sub in [r"SOFTWARE\WOW6432Node\Valve\Steam", r"SOFTWARE\Valve\Steam"] {
        if let Ok(key) = hklm.open_subkey(sub) {
            if let Ok(p) = key.get_value::<String, _>("InstallPath") {
                cands.push(PathBuf::from(p.replace('/', "\\")));
            }
        }
    }

    for c in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        cands.push(PathBuf::from(c));
    }

    // Keep existing roots, de-duplicated (case-insensitive).
    let mut out: Vec<PathBuf> = Vec::new();
    for c in cands {
        if c.exists() {
            let key = c.to_string_lossy().to_lowercase();
            if !out.iter().any(|e| e.to_string_lossy().to_lowercase() == key) {
                out.push(c);
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn steam_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn detect_steam(roots: &[PathBuf], backup_root: &Path, notes: &mut Vec<String>) -> Option<GameInfo> {
    if roots.is_empty() {
        return None;
    }

    // Gather every library across every Steam root.
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in roots {
        libs.push(root.clone());
        let vdf_path = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(txt) = std::fs::read_to_string(&vdf_path) {
            for p in vdf::parse_library_paths(&txt) {
                libs.push(PathBuf::from(p));
            }
        }
    }
    libs.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    libs.dedup_by_key(|p| p.to_string_lossy().to_lowercase());

    // Collect every resolvable KH3 install (a library may hold a stale manifest after a move).
    let mut found: Vec<GameInfo> = Vec::new();
    for lib in &libs {
        let acf = lib
            .join("steamapps")
            .join(format!("appmanifest_{}.acf", KH3_APPID));
        let Ok(txt) = std::fs::read_to_string(&acf) else {
            continue;
        };
        let Some(installdir) = vdf::read_installdir(&txt) else {
            continue;
        };
        let common = lib.join("steamapps").join("common").join(&installdir);
        match find_exe_under(&common) {
            Some(exe) => match patch::inspect(&exe, Store::Steam, backup_root) {
                Ok(gi) => {
                    if !found.iter().any(|g| g.exe_path.eq_ignore_ascii_case(&gi.exe_path)) {
                        found.push(gi);
                    }
                }
                Err(e) => notes.push(format!("Steam install found, but the exe couldn't be read: {e}")),
            },
            None => notes.push(format!(
                "A Steam manifest for KH3 was found in {}, but the exe is missing (game not fully installed?).",
                lib.display()
            )),
        }
    }

    if found.len() > 1 {
        let paths: Vec<String> = found.iter().map(|g| g.exe_path.clone()).collect();
        notes.push(format!(
            "KINGDOM HEARTS III found in {} Steam libraries ({}). Selected the most suitable — use Browse to override.",
            found.len(),
            paths.join("  |  ")
        ));
    }
    pick_best(found)
}

// ---------------------------------------------------------------------------
// Epic
// ---------------------------------------------------------------------------

fn detect_epic(backup_root: &Path, notes: &mut Vec<String>) -> Option<GameInfo> {
    let program_data = std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    let man_dir = PathBuf::from(program_data)
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    if !man_dir.exists() {
        return None; // Epic not installed — silent.
    }

    let Ok(rd) = std::fs::read_dir(&man_dir) else {
        return None;
    };

    let mut found: Vec<GameInfo> = Vec::new();
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.extension().map(|e| e.eq_ignore_ascii_case("item")).unwrap_or(false) {
            continue;
        }
        let Ok(txt) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            continue;
        };
        // Be specific: the base game's DisplayName contains "KINGDOM HEARTS III". This
        // excludes other KH titles (e.g. "KINGDOM HEARTS HD 1.5+2.5"); DLC/edition manifests
        // that don't resolve to an exe are skipped below.
        let display = v.get("DisplayName").and_then(|x| x.as_str()).unwrap_or("");
        if !display.to_uppercase().contains("KINGDOM HEARTS III") {
            continue;
        }

        let install = v.get("InstallLocation").and_then(|x| x.as_str()).unwrap_or("");
        let launch = v.get("LaunchExecutable").and_then(|x| x.as_str()).unwrap_or("");
        if install.is_empty() {
            continue;
        }

        let mut exe = PathBuf::from(install);
        if !launch.is_empty() {
            exe = exe.join(launch.replace('/', "\\"));
        }
        let resolved = if exe.exists() {
            Some(exe)
        } else {
            find_exe_under(Path::new(install))
        };

        if let Some(exe) = resolved {
            match patch::inspect(&exe, Store::Epic, backup_root) {
                Ok(gi) => {
                    if !found.iter().any(|g| g.exe_path.eq_ignore_ascii_case(&gi.exe_path)) {
                        found.push(gi);
                    }
                }
                Err(e) => notes.push(format!("Epic install found, but the exe couldn't be read: {e}")),
            }
        }
    }

    if found.len() > 1 {
        notes.push(format!(
            "KINGDOM HEARTS III matched {} Epic manifests — selected the most suitable.",
            found.len()
        ));
    }
    pick_best(found)
}

// ---------------------------------------------------------------------------
// Shared: locate the exe under an install root (handles KH3's double-nesting)
// ---------------------------------------------------------------------------

fn find_exe_under(root: &Path) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }
    // Deterministic fast paths for KH3's known layout (it double-nests on Steam:
    // <installdir>\<installdir>\Binaries\Win64\). exists() transparently resolves junctions.
    let direct = root.join("Binaries").join("Win64").join(EXE_NAME);
    if direct.exists() {
        return Some(direct);
    }
    if let Some(name) = root.file_name() {
        let nested = root.join(name).join("Binaries").join("Win64").join(EXE_NAME);
        if nested.exists() {
            return Some(nested);
        }
    }
    // Fallback: bounded walk (follow junctions, depth-capped). Pick the shallowest match
    // deterministically so a stray duplicate copy can't be chosen at random.
    let mut matches: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(6)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name().to_string_lossy().eq_ignore_ascii_case(EXE_NAME)
                && entry
                    .path()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(r"\binaries\win64\")
        })
        .map(|e| e.path().to_path_buf())
        .collect();
    matches.sort_by_key(|p| p.as_os_str().len());
    matches.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the full real-machine detection path (registry → libraryfolders →
    /// appmanifest → walk → inspect). Gated behind KH3_DUMP_DETECT so normal runs
    /// and CI stay machine-independent. Run with:
    ///   $env:KH3_DUMP_DETECT=1; cargo test dump_detect -- --nocapture
    #[test]
    fn dump_detect() {
        if std::env::var_os("KH3_DUMP_DETECT").is_none() {
            return;
        }
        let r = detect_all();
        eprintln!("steam_root: {:?}", r.steam_root);
        eprintln!("candidates: {}", r.candidates.len());
        for c in &r.candidates {
            eprintln!(
                "  store={:?} state={:?} baseline={} writable={} running={} protected={}",
                c.store, c.state, c.is_baseline, c.writable, c.running, c.on_protected_path
            );
            eprintln!("    path  = {}", c.exe_path);
            eprintln!("    size  = {}  sha = {}", c.size, c.sha256);
            eprintln!("    backup= {:?}", c.backup_path);
        }
        for n in &r.notes {
            eprintln!("  note: {}", n);
        }
        assert!(!r.candidates.is_empty(), "expected to detect the Steam install");
    }
}
