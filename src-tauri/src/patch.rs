//! The patch core: compute values, plan, apply, revert, verify, back up.
//! A faithful Rust port of the proven `patch-ultrawide.ps1` algorithm.

use crate::error::{AppError, AppResult};
use crate::model::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Hashing & hex
// ---------------------------------------------------------------------------

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    use std::fmt::Write;
    for b in digest {
        let _ = write!(s, "{:02X}", b);
    }
    s
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{:02X}", x))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Value computation
// ---------------------------------------------------------------------------

/// Compute the aspect + Hor+ FOV bytes for a target resolution.
/// `fov_override`: `Some(d)` with d>0 forces an explicit FOV; otherwise auto Hor+.
pub fn compute_values(width: u32, height: u32, fov_override: Option<f64>) -> ComputedValues {
    let (w, h) = (width.max(1), height.max(1));
    let aspect = w as f32 / h as f32;
    let aspect_bytes = aspect.to_le_bytes();
    let is_16_9 = (aspect - (16.0_f32 / 9.0_f32)).abs() < 1.0e-4;

    let fov_deg = match fov_override {
        // Guard the safety boundary: only accept a finite, sane custom FOV; otherwise auto Hor+.
        Some(f) if f.is_finite() && f > 0.0 && f <= 170.0 => f,
        _ => 2.0 * ((w as f64 / h as f64) * 9.0 / 16.0).atan() * 180.0 / std::f64::consts::PI,
    };
    let fov_bytes = (fov_deg as f32).to_le_bytes();

    ComputedValues {
        aspect,
        aspect_bytes,
        aspect_hex: to_hex(&aspect_bytes),
        fov_deg,
        fov_bytes,
        fov_hex: to_hex(&fov_bytes),
        is_16_9,
    }
}

/// Reject absurd / out-of-range resolutions before any byte is written. The UI also
/// validates, but the Rust core is the real safety boundary (compute/apply are IPC-reachable).
fn validate_dims(width: u32, height: u32) -> Option<String> {
    if !(1024..=16384).contains(&width) {
        return Some(format!("Width {width}px is outside the supported range (1024–16384)."));
    }
    if !(600..=8640).contains(&height) {
        return Some(format!("Height {height}px is outside the supported range (600–8640)."));
    }
    let r = width as f64 / height as f64;
    if !(1.6..=4.0).contains(&r) {
        return Some(format!(
            "Aspect ratio {r:.3} is outside the supported range (1.60–4.00 — 16:10 through 32:9)."
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// Byte search
// ---------------------------------------------------------------------------

/// All start offsets of `pat` in `hay` (overlapping, step 1 — parity with the
/// PowerShell reference; only matters for diagnostics since patterns are unique).
pub fn find_all(hay: &[u8], pat: &[u8]) -> Vec<usize> {
    let mut res = Vec::new();
    if pat.is_empty() || pat.len() > hay.len() {
        return res;
    }
    let finder = memchr::memmem::Finder::new(pat);
    let mut start = 0usize;
    while start + pat.len() <= hay.len() {
        match finder.find(&hay[start..]) {
            Some(pos) => {
                let abs = start + pos;
                res.push(abs);
                start = abs + 1;
            }
            None => break,
        }
    }
    res
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

fn plan_site(bytes: &[u8], e: &EditDescriptor) -> (SitePlan, Option<String>) {
    let mut pat = Vec::with_capacity(e.prefix.len() + 4);
    pat.extend_from_slice(e.prefix);
    pat.extend_from_slice(&e.old);
    let hits = find_all(bytes, &pat);
    let count = hits.len();

    let (state, offset, abort) = if count == 1 {
        (SiteState::Patch, Some((hits[0] + e.prefix.len()) as u64), None)
    } else if count == 0 {
        // Required: nothing to do (already patched). Optional: simply not present.
        (
            if e.optional { SiteState::Skipped } else { SiteState::Already },
            None,
            None,
        )
    } else if e.optional {
        (SiteState::Skipped, None, None)
    } else {
        (
            SiteState::Abort,
            None,
            Some(format!(
                "Site '{}': expected exactly one occurrence of [{}]; found {}. Aborting (build may be unexpected).",
                e.name,
                to_hex(&pat),
                count
            )),
        )
    };

    (
        SitePlan {
            name: e.name.to_string(),
            group: e.group.to_string(),
            optional: e.optional,
            kind: e.kind,
            offset,
            state,
            count,
        },
        abort,
    )
}

pub fn build_plan(bytes: &[u8], opt: &PatchOptions) -> PatchPlan {
    let computed = compute_values(opt.width, opt.height, opt.fov_degrees);

    if let Some(reason) = validate_dims(opt.width, opt.height) {
        return PatchPlan {
            computed,
            sites: Vec::new(),
            will_write: false,
            abort_reason: Some(reason),
            no_change_16_9: false,
        };
    }

    if computed.is_16_9 {
        return PatchPlan {
            computed,
            sites: Vec::new(),
            will_write: false,
            abort_reason: None,
            no_change_16_9: true,
        };
    }

    let mut sites = Vec::new();
    let mut will_write = false;
    let mut abort_reason: Option<String> = None;

    let consider = |edits: &[EditDescriptor], sites: &mut Vec<SitePlan>, will_write: &mut bool, abort_reason: &mut Option<String>| {
        for e in edits {
            let (sp, ab) = plan_site(bytes, e);
            if sp.state == SiteState::Patch {
                *will_write = true;
            }
            if abort_reason.is_none() {
                if let Some(reason) = ab {
                    *abort_reason = Some(reason);
                }
            }
            sites.push(sp);
        }
    };

    consider(DEFAULT_EDITS, &mut sites, &mut will_write, &mut abort_reason);
    if opt.include_advanced {
        consider(ADVANCED_EDITS, &mut sites, &mut will_write, &mut abort_reason);
    }

    // All-or-nothing for REQUIRED sites: a mix of patchable + already-done means an
    // unexpected/partially-modified build. Writing only the matching sites would leave
    // the game stretched (group A alone), so abort instead of silently half-patching.
    if abort_reason.is_none() {
        let req_patch = sites.iter().filter(|s| !s.optional && s.state == SiteState::Patch).count();
        let req_already = sites.iter().filter(|s| !s.optional && s.state == SiteState::Already).count();
        if req_patch > 0 && req_already > 0 {
            abort_reason = Some(format!(
                "Unexpected or partially-modified build: {} of {} required patch sites are present but {} are missing. Refusing to write a partial patch (it would leave the game stretched). Restore the original exe (Steam/Epic → Verify integrity of game files) and try again.",
                req_patch,
                req_patch + req_already,
                req_already
            ));
        }
    }

    PatchPlan {
        computed,
        sites,
        will_write,
        abort_reason,
        no_change_16_9: false,
    }
}

// ---------------------------------------------------------------------------
// File state probes
// ---------------------------------------------------------------------------

/// A running exe image is locked against write access → Windows returns
/// ERROR_SHARING_VIOLATION (32). We open for write (no truncation) and check.
pub fn is_running(exe: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(exe) {
        Ok(_) => false,
        Err(e) => e.raw_os_error() == Some(32),
    }
}

/// Can we write this file (ignoring a transient running-lock)? Access-denied (5)
/// means no (needs elevation / read-only); sharing-violation (32) means the perms
/// are fine, it's just locked right now.
pub fn probe_writable(exe: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(exe) {
        Ok(_) => true,
        Err(e) => e.raw_os_error() == Some(32),
    }
}

pub fn is_protected_path(p: &Path) -> bool {
    p.to_string_lossy().to_lowercase().contains("\\program files")
}

// ---------------------------------------------------------------------------
// Backups (kept in %LOCALAPPDATA%, not the game folder)
// ---------------------------------------------------------------------------

pub fn backup_root_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("kh3-ultrawide-patcher").join("backups")
}

fn backup_subdir(backup_root: &Path, exe: &Path) -> PathBuf {
    // Key by a hash of the exe's path so multiple installs don't collide.
    let key = sha256_bytes(exe.to_string_lossy().to_lowercase().as_bytes());
    backup_root.join(&key[..16])
}

fn exe_file_name(exe: &Path) -> String {
    exe.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| EXE_NAME.to_string())
}

/// Cross-directory recovery: scan every backup subdir for a `.orig` whose contents hash to the
/// clean baseline. Lets revert recover after the game was moved/reinstalled (which changes the
/// path-derived key, orphaning the original keyed dir).
fn scan_all_backups_for_baseline(backup_root: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(backup_root).ok()?;
    for sub in rd.flatten() {
        let subp = sub.path();
        if !subp.is_dir() {
            continue;
        }
        if let Ok(rd2) = std::fs::read_dir(&subp) {
            for ent in rd2.flatten() {
                let p = ent.path();
                if p.extension().map(|e| e == "orig").unwrap_or(false)
                    && sha256_file(&p).map(|h| h.eq_ignore_ascii_case(BASELINE_SHA)).unwrap_or(false)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn existing_backup(backup_root: &Path, exe: &Path) -> Option<String> {
    let dir = backup_subdir(backup_root, exe);
    let baseline_named = dir.join(format!("{}.{}.orig", exe_file_name(exe), &BASELINE_SHA[..7]));
    if baseline_named.exists() {
        return Some(baseline_named.to_string_lossy().to_string());
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().map(|e| e == "orig").unwrap_or(false) {
                if let Ok(md) = ent.metadata() {
                    if let Ok(m) = md.modified() {
                        if newest.as_ref().map(|(t, _)| m > *t).unwrap_or(true) {
                            newest = Some((m, p));
                        }
                    }
                }
            }
        }
    }
    if let Some((_, p)) = newest {
        return Some(p.to_string_lossy().to_string());
    }
    // Game may have moved (path-keyed dir changed): look for a clean baseline backup anywhere.
    scan_all_backups_for_baseline(backup_root).map(|p| p.to_string_lossy().to_string())
}

/// Back up only when the exe matches the clean baseline (never overwrite a good
/// backup). Returns the backup path to surface in the UI.
fn backup_if_baseline(exe: &Path, sha: &str, is_baseline: bool, backup_root: &Path) -> AppResult<Option<String>> {
    if !is_baseline {
        return Ok(existing_backup(backup_root, exe));
    }
    let dir = backup_subdir(backup_root, exe);
    std::fs::create_dir_all(&dir)?;
    let bp = dir.join(format!("{}.{}.orig", exe_file_name(exe), &sha[..7]));
    if !bp.exists() {
        std::fs::copy(exe, &bp)?;
    }
    Ok(Some(bp.to_string_lossy().to_string()))
}

// ---------------------------------------------------------------------------
// Atomic write
// ---------------------------------------------------------------------------

/// Write `data` to a temp file in the same directory, then atomically replace the
/// target. On Windows `fs::rename` maps to MoveFileExW with replace-existing.
pub fn atomic_write_replace(target: &Path, data: &[u8]) -> AppResult<()> {
    let dir = target
        .parent()
        .ok_or_else(|| AppError::Io("target has no parent directory".to_string()))?;
    let tmp = dir.join(format!("{}.uwtmp", exe_file_name(target)));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, target) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(AppError::Io(format!(
                "Couldn't replace the exe: {e}. Is the game running, or does the patcher need to run as administrator?"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Inspect
// ---------------------------------------------------------------------------

pub fn inspect(exe: &Path, store: Store, backup_root: &Path) -> AppResult<GameInfo> {
    if !exe.exists() {
        return Err(AppError::NotFound(exe.display().to_string()));
    }
    let bytes = std::fs::read(exe)?;
    let size = bytes.len() as u64;
    let sha256 = sha256_bytes(&bytes);
    let is_baseline = size == BASELINE_SIZE && sha256.eq_ignore_ascii_case(BASELINE_SHA);

    let state = if is_baseline {
        ExeState::CleanBaseline
    } else if sha256.eq_ignore_ascii_case(PATCHED_3440_SHA) {
        ExeState::AlreadyPatched
    } else {
        let mut any_old = false;
        for e in DEFAULT_EDITS {
            let mut pat = Vec::with_capacity(e.prefix.len() + 4);
            pat.extend_from_slice(e.prefix);
            pat.extend_from_slice(&e.old);
            if !find_all(&bytes, &pat).is_empty() {
                any_old = true;
                break;
            }
        }
        if any_old {
            ExeState::Patchable
        } else {
            // No required original values remain → effectively already patched.
            ExeState::AlreadyPatched
        }
    };

    let backup_path = existing_backup(backup_root, exe);
    Ok(GameInfo {
        store,
        exe_path: exe.to_string_lossy().to_string(),
        size,
        sha256,
        is_baseline,
        state,
        backup_present: backup_path.is_some(),
        backup_path,
        on_protected_path: is_protected_path(exe),
        writable: probe_writable(exe),
        running: is_running(exe),
    })
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

pub fn apply(exe: &Path, opt: &PatchOptions, backup_root: &Path) -> AppResult<PatchReport> {
    if !exe.exists() {
        return Err(AppError::NotFound(exe.display().to_string()));
    }
    if !opt.force && is_running(exe) {
        return Err(AppError::Locked(
            "KINGDOM HEARTS III appears to be running — quit the game first (the exe is locked while running).".to_string(),
        ));
    }

    let bytes = std::fs::read(exe)?;
    let size_before = bytes.len() as u64;
    let sha_before = sha256_bytes(&bytes);
    let is_baseline = sha_before.eq_ignore_ascii_case(BASELINE_SHA);

    let plan = build_plan(&bytes, opt);

    if plan.no_change_16_9 {
        return Ok(PatchReport {
            ok: true,
            size_before,
            size_after: size_before,
            size_unchanged: true,
            sha_before: sha_before.clone(),
            sha_after: sha_before,
            residual_required: 0,
            matches_known_patched: false,
            applied: Vec::new(),
            backup_path: existing_backup(backup_root, exe),
            message: "Selected resolution is 16:9 — no ultrawide change needed.".to_string(),
        });
    }

    if let Some(reason) = &plan.abort_reason {
        return Err(AppError::AbortMultiMatch(reason.clone()));
    }

    // Defense in depth: never write the UI-boxing value (unreachable; 16:9 is
    // already short-circuited above).
    if plan.computed.aspect_bytes == DANGER_UI {
        return Err(AppError::Danger(
            "Refusing to write the 16:9 UI-boxing value (39 8E E3 3F).".to_string(),
        ));
    }

    if !plan.will_write {
        return Ok(PatchReport {
            ok: true,
            size_before,
            size_after: size_before,
            size_unchanged: true,
            sha_before: sha_before.clone(),
            sha_after: sha_before.clone(),
            residual_required: 0,
            matches_known_patched: sha_before.eq_ignore_ascii_case(PATCHED_3440_SHA),
            applied: plan.sites.clone(),
            backup_path: existing_backup(backup_root, exe),
            message: "All target sites are already patched — no changes made.".to_string(),
        });
    }

    let backup_path = backup_if_baseline(exe, &sha_before, is_baseline, backup_root)?;

    let mut patched = bytes;
    for sp in &plan.sites {
        if sp.state == SiteState::Patch {
            if let Some(off) = sp.offset {
                let off = off as usize;
                let nb = match sp.kind {
                    EditKind::Aspect => plan.computed.aspect_bytes,
                    EditKind::Fov => plan.computed.fov_bytes,
                };
                patched[off..off + 4].copy_from_slice(&nb);
            }
        }
    }

    atomic_write_replace(exe, &patched)?;

    // Verify.
    let after = std::fs::read(exe)?;
    let size_after = after.len() as u64;
    let sha_after = sha256_bytes(&after);
    let mut residual_required = 0usize;
    for e in DEFAULT_EDITS {
        let mut pat = Vec::with_capacity(e.prefix.len() + 4);
        pat.extend_from_slice(e.prefix);
        pat.extend_from_slice(&e.old);
        residual_required += find_all(&after, &pat).len();
    }
    // Positively confirm every planned write actually landed (default AND advanced sites) —
    // not merely that the old bytes are gone.
    let mut writes_confirmed = true;
    for sp in &plan.sites {
        if sp.state == SiteState::Patch {
            if let Some(off) = sp.offset {
                let off = off as usize;
                let expected = match sp.kind {
                    EditKind::Aspect => plan.computed.aspect_bytes,
                    EditKind::Fov => plan.computed.fov_bytes,
                };
                if off + 4 > after.len() || after[off..off + 4] != expected {
                    writes_confirmed = false;
                }
            }
        }
    }
    let size_unchanged = size_after == size_before;
    let ok = size_unchanged && residual_required == 0 && writes_confirmed;
    let patched_count = plan.sites.iter().filter(|s| s.state == SiteState::Patch).count();

    let message = if ok {
        format!(
            "Patched {} site(s). True {}×{} ultrawide (Hor+, FOV {:.2}°). Launch Borderless Fullscreen at {}×{}.",
            patched_count, opt.width, opt.height, plan.computed.fov_deg, opt.width, opt.height
        )
    } else {
        "Verification was unexpected — consider reverting and re-checking.".to_string()
    };

    Ok(PatchReport {
        ok,
        size_before,
        size_after,
        size_unchanged,
        sha_before,
        sha_after: sha_after.clone(),
        residual_required,
        matches_known_patched: sha_after.eq_ignore_ascii_case(PATCHED_3440_SHA),
        applied: plan.sites,
        backup_path,
        message,
    })
}

// ---------------------------------------------------------------------------
// Revert
// ---------------------------------------------------------------------------

pub fn revert(exe: &Path, backup_root: &Path) -> AppResult<PatchReport> {
    if !exe.exists() {
        return Err(AppError::NotFound(exe.display().to_string()));
    }
    if is_running(exe) {
        return Err(AppError::Locked(
            "KINGDOM HEARTS III appears to be running — quit the game first.".to_string(),
        ));
    }
    let dir = backup_subdir(backup_root, exe);

    // Prefer a backup whose contents hash to the clean baseline; else the newest.
    let mut chosen: Option<PathBuf> = None;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().map(|e| e == "orig").unwrap_or(false) {
                if sha256_file(&p).map(|h| h.eq_ignore_ascii_case(BASELINE_SHA)).unwrap_or(false) {
                    chosen = Some(p);
                    break;
                }
                if let Ok(md) = ent.metadata() {
                    if let Ok(m) = md.modified() {
                        if newest.as_ref().map(|(t, _)| m > *t).unwrap_or(true) {
                            newest = Some((m, p.clone()));
                        }
                    }
                }
            }
        }
    }
    // Cross-directory recovery (game moved/reinstalled → different path key).
    if chosen.is_none() {
        chosen = scan_all_backups_for_baseline(backup_root);
    }
    if chosen.is_none() {
        chosen = newest.map(|(_, p)| p);
    }
    let chosen = chosen.ok_or_else(|| {
        AppError::NoBackup(
            "No backup found. You can also restore via Steam → Properties → Installed Files → Verify integrity of game files.".to_string(),
        )
    })?;

    let sha_before = sha256_file(exe).unwrap_or_default();
    let size_before = std::fs::metadata(exe).map(|m| m.len()).unwrap_or(0);

    let data = std::fs::read(&chosen)?;
    // Sanity-check the backup before clobbering the live exe with it.
    if data.len() < 1024 || !data.starts_with(b"MZ") {
        return Err(AppError::NoBackup(format!(
            "Backup '{}' doesn't look like a valid Windows executable — refusing to restore it.",
            chosen.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        )));
    }
    atomic_write_replace(exe, &data)?;

    let sha_after = sha256_file(exe)?;
    let size_after = std::fs::metadata(exe).map(|m| m.len()).unwrap_or(0);
    let restored_clean = sha_after.eq_ignore_ascii_case(BASELINE_SHA);
    let message = if restored_clean {
        "Exe restored to the clean baseline.".to_string()
    } else {
        format!(
            "Restored from backup '{}', but it doesn't match the known baseline hash (expected if it was made from a newer Steam build).",
            chosen.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        )
    };

    Ok(PatchReport {
        ok: true,
        size_before,
        size_after,
        size_unchanged: true,
        sha_before,
        sha_after,
        residual_required: 0,
        matches_known_patched: false,
        applied: Vec::new(),
        backup_path: Some(chosen.to_string_lossy().to_string()),
        message,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(w: u32, h: u32, advanced: bool) -> PatchOptions {
        PatchOptions { width: w, height: h, fov_degrees: None, include_advanced: advanced, force: true }
    }

    #[test]
    fn computed_byte_table() {
        let c = compute_values(3440, 1440, None);
        assert_eq!(c.aspect_bytes, [0x8E, 0xE3, 0x18, 0x40], "3440x1440 aspect");
        assert_eq!(c.fov_bytes, [0x25, 0x60, 0xD5, 0x42], "3440x1440 Hor+ FOV");
        assert!(!c.is_16_9);
        assert!((c.fov_deg - 106.69).abs() < 0.1, "fov ~106.69, got {}", c.fov_deg);

        // 16:9 resolutions short-circuit and never produce the DANGER bytes via a write.
        assert!(compute_values(1920, 1080, None).is_16_9);
        assert!(compute_values(2560, 1440, None).is_16_9);
        assert_eq!(compute_values(1920, 1080, None).aspect_bytes, DANGER_UI);

        // Aspect-sharing presets compute identical bytes.
        assert_eq!(
            compute_values(2560, 1080, None).aspect_bytes,
            compute_values(5120, 2160, None).aspect_bytes
        );
    }

    /// Build a synthetic buffer containing each given edit's (prefix ++ old)
    /// exactly once, separated by filler that can't form a pattern.
    fn synth(edits: &[&EditDescriptor]) -> Vec<u8> {
        let mut v = vec![0x90u8; 8];
        for e in edits {
            v.extend_from_slice(e.prefix);
            v.extend_from_slice(&e.old);
            v.extend_from_slice(&[0x90u8; 8]);
        }
        v
    }

    #[test]
    fn plan_all_default_sites_patch() {
        let refs: Vec<&EditDescriptor> = DEFAULT_EDITS.iter().collect();
        let buf = synth(&refs);
        let plan = build_plan(&buf, &opt(3440, 1440, false));
        assert!(plan.abort_reason.is_none());
        assert!(plan.will_write);
        assert_eq!(plan.sites.len(), 7);
        assert!(plan.sites.iter().all(|s| s.state == SiteState::Patch));
    }

    #[test]
    fn plan_16_9_is_noop() {
        let refs: Vec<&EditDescriptor> = DEFAULT_EDITS.iter().collect();
        let buf = synth(&refs);
        let plan = build_plan(&buf, &opt(2560, 1440, false));
        assert!(plan.no_change_16_9);
        assert!(!plan.will_write);
    }

    #[test]
    fn required_duplicate_aborts() {
        let refs: Vec<&EditDescriptor> = DEFAULT_EDITS.iter().collect();
        let mut buf = synth(&refs);
        // Append a second copy of the bare render-aspect value (a required site).
        buf.extend_from_slice(&OLD_RENDER_169);
        buf.extend_from_slice(&[0x90u8; 8]);
        let plan = build_plan(&buf, &opt(3440, 1440, false));
        assert!(plan.abort_reason.is_some(), "duplicate required site must abort");
    }

    #[test]
    fn optional_duplicate_skips_not_aborts() {
        let mut refs: Vec<&EditDescriptor> = DEFAULT_EDITS.iter().collect();
        refs.extend(ADVANCED_EDITS.iter());
        let mut buf = synth(&refs);
        // Duplicate one advanced site's pattern.
        let adv = &ADVANCED_EDITS[0];
        buf.extend_from_slice(adv.prefix);
        buf.extend_from_slice(&adv.old);
        buf.extend_from_slice(&[0x90u8; 8]);
        let plan = build_plan(&buf, &opt(3440, 1440, true));
        assert!(plan.abort_reason.is_none(), "optional duplicate must not abort");
        let dup = plan.sites.iter().find(|s| s.name == adv.name).unwrap();
        assert_eq!(dup.state, SiteState::Skipped);
    }

    #[test]
    fn apply_and_verify_on_synthetic_file() {
        let refs: Vec<&EditDescriptor> = DEFAULT_EDITS.iter().collect();
        let buf = synth(&refs);
        let tmp_dir = std::env::temp_dir().join(format!("kh3uw_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let exe = tmp_dir.join("fake.exe");
        std::fs::write(&exe, &buf).unwrap();
        let backup_root = tmp_dir.join("backups");

        let report = apply(&exe, &opt(3440, 1440, false), &backup_root).unwrap();
        assert!(report.ok, "apply should verify ok: {}", report.message);
        assert_eq!(report.residual_required, 0);
        assert!(report.size_unchanged);

        // Re-applying is idempotent → already patched, no write.
        let again = apply(&exe, &opt(3440, 1440, false), &backup_root).unwrap();
        assert!(again.ok);
        assert!(again.message.to_lowercase().contains("already"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn partial_required_build_aborts() {
        // Only the bare render-aspect (group A) is present; the camera sites are absent →
        // mixed required states must abort instead of writing a stretched partial patch.
        let mut buf = vec![0x90u8; 8];
        buf.extend_from_slice(&OLD_RENDER_169);
        buf.extend_from_slice(&[0x90u8; 8]);
        let plan = build_plan(&buf, &opt(3440, 1440, false));
        assert!(plan.abort_reason.is_some(), "mixed required sites must abort");
    }

    #[test]
    fn invalid_dimensions_abort() {
        assert!(build_plan(&[0u8; 16], &opt(99999, 1440, false)).abort_reason.is_some());
        assert!(build_plan(&[0u8; 16], &opt(1, 1440, false)).abort_reason.is_some());
        assert!(build_plan(&[0u8; 16], &opt(3840, 100, false)).abort_reason.is_some());
    }

    /// Real-bytes golden test. Set `KH3_EXE_COPY` to a CLEAN baseline exe (e.g. the
    /// project's `_backup\*.orig`). The file is copied to a temp dir and never
    /// modified in place. Skips silently when the env var is unset, so normal
    /// `cargo test` and CI stay machine-independent and PII-free.
    #[test]
    fn golden_real_exe() {
        let Some(src) = std::env::var_os("KH3_EXE_COPY") else {
            eprintln!("golden_real_exe: KH3_EXE_COPY not set — skipping");
            return;
        };
        let src = std::path::PathBuf::from(src);
        assert!(src.exists(), "KH3_EXE_COPY does not exist: {}", src.display());

        let tmp_dir = std::env::temp_dir().join(format!("kh3uw_golden_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let exe = tmp_dir.join("KINGDOM HEARTS III.exe");
        std::fs::copy(&src, &exe).unwrap();
        let backup_root = tmp_dir.join("backups");

        assert_eq!(
            sha256_file(&exe).unwrap(),
            BASELINE_SHA,
            "source must be the clean baseline"
        );

        // Patch 3440x1440 (auto Hor+) → must reproduce the golden patched build byte-for-byte.
        let rep = apply(&exe, &opt(3440, 1440, false), &backup_root).unwrap();
        assert!(rep.ok, "patch verify failed: {}", rep.message);
        assert_eq!(rep.sha_after, PATCHED_3440_SHA, "patched bytes must match golden SHA");
        assert!(rep.matches_known_patched);

        // Revert → back to the clean baseline.
        let rev = revert(&exe, &backup_root).unwrap();
        assert_eq!(rev.sha_after, BASELINE_SHA, "revert must restore the baseline");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
