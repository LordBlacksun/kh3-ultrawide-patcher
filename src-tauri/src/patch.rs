//! The patch core: compute values, plan, apply, revert, verify, back up.

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

/// Compute the aspect bytes for a target resolution (+ the Hor+ angle of a 90° camera, for display).
pub fn compute_values(width: u32, height: u32) -> ComputedValues {
    let (w, h) = (width.max(1), height.max(1));
    let aspect = w as f32 / h as f32;
    let aspect_bytes = aspect.to_le_bytes();
    let is_16_9 = (aspect - (16.0_f32 / 9.0_f32)).abs() < 1.0e-4;
    let hor_plus_90_deg = (2.0 * ((w as f64 / h as f64) * 9.0 / 16.0).atan()).to_degrees();

    ComputedValues {
        aspect,
        aspect_bytes,
        aspect_hex: to_hex(&aspect_bytes),
        hor_plus_90_deg,
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

/// All start offsets of `pat` in `hay` (overlapping, step 1).
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

/// Parse a byte pattern like `"F3 0F ?? 28"`; `??` matches any byte.
pub fn parse_pattern(spec: &str) -> Vec<Option<u8>> {
    spec.split_whitespace()
        .map(|t| if t == "??" { None } else { Some(u8::from_str_radix(t, 16).expect("bad pattern byte")) })
        .collect()
}

/// All start offsets where `pat` matches. Anchors on the longest run of fixed bytes.
pub fn find_pattern(hay: &[u8], pat: &[Option<u8>]) -> Vec<usize> {
    let (mut anchor_at, mut anchor_len, mut i) = (0, 0, 0);
    while i < pat.len() {
        let s = i;
        while i < pat.len() && pat[i].is_some() {
            i += 1;
        }
        if i - s > anchor_len {
            anchor_at = s;
            anchor_len = i - s;
        }
        i += 1;
    }
    if anchor_len == 0 || pat.len() > hay.len() {
        return Vec::new();
    }
    let anchor: Vec<u8> = pat[anchor_at..anchor_at + anchor_len].iter().map(|b| b.unwrap()).collect();
    find_all(hay, &anchor)
        .into_iter()
        .filter_map(|a| a.checked_sub(anchor_at))
        .filter(|&s| {
            s + pat.len() <= hay.len() && pat.iter().enumerate().all(|(k, p)| p.map_or(true, |v| hay[s + k] == v))
        })
        .collect()
}

fn read_i32(b: &[u8], o: usize) -> Option<i32> {
    b.get(o..o + 4).map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_u32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_u16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

/// Little-endian rel32 from the end of an instruction (`next`) to `target`.
fn rel32(next: u32, target: u32) -> [u8; 4] {
    ((target as i64 - next as i64) as i32).to_le_bytes()
}

// ---------------------------------------------------------------------------
// PE image (just enough to map offsets, find .text and the .pdata function table)
// ---------------------------------------------------------------------------

struct Section {
    name: [u8; 8],
    va: u32,
    raw_ptr: u32,
    raw_size: u32,
}

struct PeImage {
    sections: Vec<Section>,
    exception_rva: u32,
    exception_size: u32,
}

impl PeImage {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.get(0..2)? != b"MZ" {
            return None;
        }
        let pe = read_u32(b, 0x3C)? as usize;
        if b.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let nsec = read_u16(b, pe + 6)? as usize;
        let opt_size = read_u16(b, pe + 20)? as usize;
        let opt = pe + 24;
        if read_u16(b, opt)? != 0x20B {
            return None; // not PE32+
        }
        let exception_dir = opt + 112 + 3 * 8;
        let mut sections = Vec::with_capacity(nsec);
        for i in 0..nsec {
            let o = opt + opt_size + i * 40;
            sections.push(Section {
                name: b.get(o..o + 8)?.try_into().ok()?,
                va: read_u32(b, o + 12)?,
                raw_size: read_u32(b, o + 16)?,
                raw_ptr: read_u32(b, o + 20)?,
            });
        }
        Some(PeImage {
            sections,
            exception_rva: read_u32(b, exception_dir)?,
            exception_size: read_u32(b, exception_dir + 4)?,
        })
    }

    fn off_to_rva(&self, off: usize) -> Option<u32> {
        let off = off as u64;
        self.sections
            .iter()
            .find(|s| off >= s.raw_ptr as u64 && off < s.raw_ptr as u64 + s.raw_size as u64)
            .map(|s| s.va + (off - s.raw_ptr as u64) as u32)
    }

    fn rva_to_off(&self, rva: u32) -> Option<usize> {
        self.sections
            .iter()
            .find(|s| rva >= s.va && rva - s.va < s.raw_size)
            .map(|s| (s.raw_ptr + (rva - s.va)) as usize)
    }

    fn section(&self, name: &[u8]) -> Option<&Section> {
        self.sections.iter().find(|s| s.name.split(|&c| c == 0).next() == Some(name))
    }

    /// (begin, end) RVAs of every function in the exception directory (.pdata).
    fn functions(&self, b: &[u8]) -> Vec<(u32, u32)> {
        let Some(start) = self.rva_to_off(self.exception_rva) else {
            return Vec::new();
        };
        (0..self.exception_size as usize / 12)
            .filter_map(|i| Some((read_u32(b, start + i * 12)?, read_u32(b, start + i * 12 + 4)?)))
            .filter(|&(begin, _)| begin != 0)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

fn site(name: &str, group: &str, kind: EditKind) -> SitePlan {
    SitePlan {
        name: name.to_string(),
        group: group.to_string(),
        kind,
        offset: None,
        state: SiteState::Abort,
        count: 0,
        writes: Vec::new(),
    }
}

fn plan_aspect(bytes: &[u8], e: &EditDescriptor, computed: &ComputedValues) -> (SitePlan, Option<String>) {
    let mut pat = Vec::with_capacity(e.prefix.len() + 4);
    pat.extend_from_slice(e.prefix);
    pat.extend_from_slice(&e.old);
    let hits = find_all(bytes, &pat);
    let mut sp = site(e.name, &e.group.to_string(), EditKind::Aspect);
    sp.count = hits.len();
    match hits.len() {
        1 => {
            let off = hits[0] + e.prefix.len();
            sp.state = SiteState::Patch;
            sp.offset = Some(off as u64);
            sp.writes = vec![(off, computed.aspect_bytes.to_vec())];
            (sp, None)
        }
        0 => {
            sp.state = SiteState::Already;
            (sp, None)
        }
        n => (
            sp,
            Some(format!(
                "Site '{}': expected exactly one occurrence of [{}]; found {}. Aborting (build may be unexpected).",
                e.name,
                to_hex(&pat),
                n
            )),
        ),
    }
}

/// The two leaf routines + the 9/16 constant, placed at RVA `cave`.
/// post_c: tan *= ViewInfo.AspectRatio * 9/16, then the displaced `movss xmm6,[1.0]`.
/// post_u: tan *= xmm11 (W/H, or 1) * 9/16, then the displaced `movaps xmm1,xmm6 ; mov [rbp-4Dh],0`.
fn projection_routines(cave: u32, one_rva: u32) -> Vec<u8> {
    let (post_u, k) = (cave + PROJ_POST_U_OFFSET, cave + PROJ_CONST_OFFSET);
    let mut v = Vec::with_capacity(PROJ_ROUTINE_LEN);
    v.extend_from_slice(&[0xF3, 0x0F, 0x59, 0x43, 0x28]); // mulss xmm0, [rbx+28h]
    v.extend_from_slice(&[0xF3, 0x0F, 0x59, 0x05]); // mulss xmm0, [rip -> 9/16]
    v.extend_from_slice(&rel32(cave + 13, k));
    v.extend_from_slice(&[0xF3, 0x0F, 0x10, 0x35]); // movss xmm6, [rip -> 1.0]
    v.extend_from_slice(&rel32(cave + 21, one_rva));
    v.push(0xC3); // ret
    v.extend_from_slice(&[0xF3, 0x41, 0x0F, 0x59, 0xC3]); // mulss xmm0, xmm11
    v.extend_from_slice(&[0xF3, 0x0F, 0x59, 0x05]); // mulss xmm0, [rip -> 9/16]
    v.extend_from_slice(&rel32(post_u + 13, k));
    v.extend_from_slice(&[0x0F, 0x28, 0xCE]); // movaps xmm1, xmm6
    v.extend_from_slice(&[0x48, 0xC7, 0x45, 0xB3, 0x00, 0x00, 0x00, 0x00]); // mov qword [rbp-4Dh], 0
    v.push(0xC3); // ret
    v.extend_from_slice(&(9.0_f32 / 16.0).to_le_bytes());
    debug_assert_eq!(v.len(), PROJ_ROUTINE_LEN);
    v
}

/// Start RVA for the routines: inside the int3 run (>= PROJ_MIN_PADDING bytes, outside every
/// .pdata function) nearest to `near_rva`.
fn find_padding(bytes: &[u8], pe: &PeImage, near_rva: u32) -> Option<u32> {
    let text = pe.section(b".text")?;
    let base = text.raw_ptr as usize;
    let data = bytes.get(base..base + text.raw_size as usize)?;
    let functions = pe.functions(bytes);
    let mut best: Option<(u64, u32)> = None;
    let mut i = 0;
    while let Some(p) = memchr::memchr(0xCC, &data[i..]) {
        let start = i + p;
        let mut end = start;
        while end < data.len() && data[end] == 0xCC {
            end += 1;
        }
        i = end;
        if end - start < PROJ_MIN_PADDING {
            continue;
        }
        let (r0, r1) = (text.va + start as u32, text.va + end as u32);
        if functions.iter().any(|&(begin, fn_end)| begin < r1 && fn_end > r0) {
            continue;
        }
        let distance = (r0 as i64 - near_rva as i64).unsigned_abs();
        if best.map_or(true, |(d, _)| distance < d) {
            best = Some((distance, r0));
        }
    }
    best.map(|(_, r0)| r0 + PROJ_PADDING_LEAD)
}

fn window_contains(bytes: &[u8], start: usize, needle: &[u8]) -> bool {
    let end = (start + PROJ_CHECK_WINDOW).min(bytes.len());
    bytes.get(start..end).map_or(false, |w| memchr::memmem::find(w, needle).is_some())
}

fn holds_one(bytes: &[u8], pe: &PeImage, rva: u32) -> bool {
    pe.rva_to_off(rva).and_then(|o| bytes.get(o..o + 4)) == Some(&1.0_f32.to_le_bytes()[..])
}

fn plan_projection(bytes: &[u8]) -> (SitePlan, Option<String>) {
    let mut sp = site("Hor+ projection fix (every camera)", "P", EditKind::Projection);
    let unrecognized = |sp: SitePlan, why: &str| {
        (sp, Some(format!("Hor+ projection fix: {why}. Aborting (build may be unexpected).")))
    };
    let Some(pe) = PeImage::parse(bytes) else {
        return unrecognized(sp, "not a 64-bit Windows executable");
    };
    let c_old = find_pattern(bytes, &parse_pattern(PROJ_CONSTRAINED_OLD));
    let u_old = find_pattern(bytes, &parse_pattern(PROJ_UNCONSTRAINED_OLD));
    let c_new = find_pattern(bytes, &parse_pattern(PROJ_CONSTRAINED_NEW));
    let u_new = find_pattern(bytes, &parse_pattern(PROJ_UNCONSTRAINED_NEW));
    sp.count = c_old.len() + u_old.len() + c_new.len() + u_new.len();

    if c_old.len() == 1 && u_old.len() == 1 && c_new.is_empty() && u_new.is_empty() {
        let (c_site, u_site) = (c_old[0] + PROJ_SITE_OFFSET, u_old[0] + PROJ_SITE_OFFSET);
        if !window_contains(bytes, c_old[0], PROJ_CHECK_CONSTRAINED)
            || !window_contains(bytes, u_old[0], PROJ_CHECK_UNCONSTRAINED)
        {
            return unrecognized(sp, "projection code layout differs from the known build");
        }
        let (Some(c_rva), Some(u_rva), Some(disp)) =
            (pe.off_to_rva(c_site), pe.off_to_rva(u_site), read_i32(bytes, c_site + 4))
        else {
            return unrecognized(sp, "sites outside the image");
        };
        let one_rva = (c_rva as i64 + 8 + disp as i64) as u32;
        if !holds_one(bytes, &pe, one_rva) {
            return unrecognized(sp, "displaced instruction doesn't load 1.0");
        }
        let Some((cave, cave_off)) = find_padding(bytes, &pe, c_rva).and_then(|r| Some((r, pe.rva_to_off(r)?))) else {
            return unrecognized(sp, "no unused padding for the routines");
        };
        let mut call_c = vec![0xE8];
        call_c.extend_from_slice(&rel32(c_rva + 5, cave));
        call_c.extend_from_slice(&[0x0F, 0x1F, 0x00]);
        let mut call_u = vec![0xE8];
        call_u.extend_from_slice(&rel32(u_rva + 5, cave + PROJ_POST_U_OFFSET));
        call_u.extend_from_slice(&[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00]);
        sp.state = SiteState::Patch;
        sp.offset = Some(c_site as u64);
        sp.writes = vec![(cave_off, projection_routines(cave, one_rva)), (c_site, call_c), (u_site, call_u)];
        return (sp, None);
    }

    if c_new.len() == 1 && u_new.len() == 1 && c_old.is_empty() && u_old.is_empty() {
        let (c_site, u_site) = (c_new[0] + PROJ_SITE_OFFSET, u_new[0] + PROJ_SITE_OFFSET);
        let already = (|| {
            let (c_rva, u_rva) = (pe.off_to_rva(c_site)?, pe.off_to_rva(u_site)?);
            let cave = (c_rva as i64 + 5 + read_i32(bytes, c_site + 1)? as i64) as u32;
            let cave_u = (u_rva as i64 + 5 + read_i32(bytes, u_site + 1)? as i64) as u32;
            let cave_off = pe.rva_to_off(cave)?;
            let one_rva = (cave as i64 + 21 + read_i32(bytes, cave_off + 17)? as i64) as u32;
            Some(
                cave_u == cave + PROJ_POST_U_OFFSET
                    && holds_one(bytes, &pe, one_rva)
                    && bytes.get(cave_off..cave_off + PROJ_ROUTINE_LEN) == Some(&projection_routines(cave, one_rva)[..]),
            )
        })();
        if already == Some(true) {
            sp.state = SiteState::Already;
            sp.offset = Some(c_site as u64);
            return (sp, None);
        }
        return unrecognized(sp, "patched call sites don't lead to the expected routines");
    }

    unrecognized(sp, "projection code not found")
}

/// Edits left by older patcher versions: restored only when the surrounding bytes match exactly
/// once and the value between them isn't the original. Never aborts.
fn plan_restores(bytes: &[u8]) -> Vec<SitePlan> {
    let mut out = Vec::new();
    for r in LEGACY_RESTORES {
        let mut values = find_all(bytes, r.prefix)
            .into_iter()
            .map(|p| p + r.prefix.len())
            .filter(|&v| bytes.get(v + 4..v + 4 + r.suffix.len()) == Some(r.suffix));
        let (Some(v), None) = (values.next(), values.next()) else {
            continue;
        };
        if bytes[v..v + 4] != r.original {
            let mut sp = site(r.name, "R", EditKind::Restore);
            sp.state = SiteState::Patch;
            sp.offset = Some(v as u64);
            sp.count = 1;
            sp.writes = vec![(v, r.original.to_vec())];
            out.push(sp);
        }
    }
    out
}

pub fn build_plan(bytes: &[u8], opt: &PatchOptions) -> PatchPlan {
    let computed = compute_values(opt.width, opt.height);

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
    let mut abort_reason: Option<String> = None;
    for e in ASPECT_EDITS {
        let (sp, ab) = plan_aspect(bytes, e, &computed);
        if abort_reason.is_none() {
            abort_reason = ab;
        }
        sites.push(sp);
    }

    // All-or-nothing for the aspect edits: a mix of patchable + already-done means an
    // unexpected/partially-modified build. Writing only the matching sites would leave
    // the game stretched (group A alone), so abort instead of silently half-patching.
    if abort_reason.is_none() {
        let patch = sites.iter().filter(|s| s.state == SiteState::Patch).count();
        let already = sites.iter().filter(|s| s.state == SiteState::Already).count();
        if patch > 0 && already > 0 {
            abort_reason = Some(format!(
                "Unexpected or partially-modified build: {} of {} aspect edit sites are present but {} are missing. Refusing to write a partial patch (it would leave the game stretched). Restore the original exe (Steam/Epic → Verify integrity of game files) and try again.",
                patch,
                patch + already,
                already
            ));
        }
    }

    let (projection, ab) = plan_projection(bytes);
    if abort_reason.is_none() {
        abort_reason = ab;
    }
    sites.push(projection);
    sites.extend(plan_restores(bytes));
    let will_write = sites.iter().any(|s| s.state == SiteState::Patch);

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

    // The resolution only affects the values written, not which sites are pending.
    let plan = build_plan(&bytes, &PatchOptions { width: 3440, height: 1440, force: true });
    let aspect_done = plan
        .sites
        .iter()
        .filter(|s| s.kind == EditKind::Aspect)
        .all(|s| s.state == SiteState::Already);
    let projection_done = plan
        .sites
        .iter()
        .any(|s| s.kind == EditKind::Projection && s.state == SiteState::Already);
    let legacy_edits = plan
        .sites
        .iter()
        .filter(|s| s.kind == EditKind::Restore && s.state == SiteState::Patch)
        .count();

    let state = if is_baseline {
        ExeState::CleanBaseline
    } else if aspect_done && projection_done && legacy_edits == 0 {
        ExeState::AlreadyPatched
    } else if aspect_done {
        ExeState::OutdatedPatch
    } else {
        ExeState::Patchable
    };

    let backup_path = existing_backup(backup_root, exe);
    Ok(GameInfo {
        store,
        exe_path: exe.to_string_lossy().to_string(),
        size,
        sha256,
        is_baseline,
        state,
        legacy_edits,
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
            message: "All edits are already in place — no changes made.".to_string(),
        });
    }

    let backup_path = backup_if_baseline(exe, &sha_before, is_baseline, backup_root)?;

    let pending = || plan.sites.iter().filter(|s| s.state == SiteState::Patch);
    let mut patched = bytes;
    for (off, data) in pending().flat_map(|s| s.writes.iter()) {
        patched[*off..*off + data.len()].copy_from_slice(data);
    }

    atomic_write_replace(exe, &patched)?;

    // Verify: every write landed, and a fresh plan of the result has nothing left to do.
    let after = std::fs::read(exe)?;
    let size_after = after.len() as u64;
    let sha_after = sha256_bytes(&after);
    let writes_confirmed = pending()
        .flat_map(|s| s.writes.iter())
        .all(|(off, data)| after.get(*off..*off + data.len()) == Some(data.as_slice()));
    let replan = build_plan(&after, opt);
    let residual_required = replan.sites.iter().filter(|s| s.state != SiteState::Already).count();
    let size_unchanged = size_after == size_before;
    let ok = size_unchanged && writes_confirmed && residual_required == 0 && replan.abort_reason.is_none();
    let restored = pending().filter(|s| s.kind == EditKind::Restore).count();
    let patched_count = pending().count() - restored;

    let message = if !ok {
        "Verification was unexpected — consider reverting and re-checking.".to_string()
    } else if patched_count == 0 {
        format!("Undid {restored} edit(s) left by an older patcher version. Your ultrawide patch is otherwise unchanged.")
    } else {
        let mut m = format!(
            "Patched {} edit(s). True {}×{} ultrawide with Hor+ on every camera. Launch at {}×{} (Borderless Fullscreen, or Fullscreen when HDR is on).",
            patched_count, opt.width, opt.height, opt.width, opt.height
        );
        if restored > 0 {
            m.push_str(&format!(" Also replaced {restored} edit(s) from an older patcher version."));
        }
        m
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

    fn opt(w: u32, h: u32) -> PatchOptions {
        PatchOptions { width: w, height: h, force: true }
    }

    fn put_u16(b: &mut [u8], o: usize, v: u16) {
        b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn put_u32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A pattern's bytes with wildcards as zero.
    fn concrete(spec: &str) -> Vec<u8> {
        parse_pattern(spec).into_iter().map(|b| b.unwrap_or(0)).collect()
    }

    const SYN_C: usize = 0x1100;
    const SYN_U: usize = 0x1200;
    const SYN_ONE: u32 = 0x2000;
    const SYN_RUN: u32 = 0x1800;

    /// Minimal PE32+ image, file offsets == RVAs: .text @0x1000, .rdata @0x2000, .pdata @0x3000.
    /// Holds the 4 aspect sites, both projection sites with their follow-up checks, the 1.0
    /// constant, a 0x100-byte int3 run at 0x1800, and one .pdata function over 0x1000..0x1300.
    fn synth_exe() -> Vec<u8> {
        let mut b = vec![0u8; 0x3100];
        b[0x1000..0x2000].fill(0x90);
        b[0..2].copy_from_slice(b"MZ");
        put_u32(&mut b, 0x3C, 0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        put_u16(&mut b, 0x86, 3);
        put_u16(&mut b, 0x94, 0xF0);
        let opt = 0x98;
        put_u16(&mut b, opt, 0x20B);
        put_u32(&mut b, opt + 112 + 24, 0x3000);
        put_u32(&mut b, opt + 112 + 28, 12);
        for (i, (name, va, size)) in [(b".text\0\0\0", 0x1000u32, 0x1000u32), (b".rdata\0\0", 0x2000, 0x1000), (b".pdata\0\0", 0x3000, 0x100)]
            .iter()
            .enumerate()
        {
            let o = opt + 0xF0 + i * 40;
            b[o..o + 8].copy_from_slice(&name[..]);
            put_u32(&mut b, o + 8, *size);
            put_u32(&mut b, o + 12, *va);
            put_u32(&mut b, o + 16, *size);
            put_u32(&mut b, o + 20, *va);
        }
        for (i, e) in ASPECT_EDITS.iter().enumerate().skip(1) {
            let o = 0x1010 + i * 0x20;
            b[o..o + e.prefix.len()].copy_from_slice(e.prefix);
            b[o + e.prefix.len()..o + e.prefix.len() + 4].copy_from_slice(&e.old);
        }
        b[0x2010..0x2014].copy_from_slice(&OLD_RENDER_169);
        b[SYN_ONE as usize..SYN_ONE as usize + 4].copy_from_slice(&1.0_f32.to_le_bytes());
        let c = concrete(PROJ_CONSTRAINED_OLD);
        b[SYN_C..SYN_C + c.len()].copy_from_slice(&c);
        let movss = SYN_C + PROJ_SITE_OFFSET;
        b[movss + 4..movss + 8].copy_from_slice(&rel32(movss as u32 + 8, SYN_ONE));
        b[SYN_C + 0x40..SYN_C + 0x40 + PROJ_CHECK_CONSTRAINED.len()].copy_from_slice(PROJ_CHECK_CONSTRAINED);
        let u = concrete(PROJ_UNCONSTRAINED_OLD);
        b[SYN_U..SYN_U + u.len()].copy_from_slice(&u);
        b[SYN_U + 0x40..SYN_U + 0x40 + PROJ_CHECK_UNCONSTRAINED.len()].copy_from_slice(PROJ_CHECK_UNCONSTRAINED);
        b[SYN_RUN as usize..SYN_RUN as usize + 0x100].fill(0xCC);
        put_u32(&mut b, 0x3000, 0x1000);
        put_u32(&mut b, 0x3004, 0x1300);
        b
    }

    fn temp_exe(tag: &str, data: &[u8]) -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("kh3uw_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("KINGDOM HEARTS III.exe");
        std::fs::write(&exe, data).unwrap();
        let backups = dir.join("backups");
        (dir, exe, backups)
    }

    #[test]
    fn computed_byte_table() {
        let c = compute_values(3440, 1440);
        assert_eq!(c.aspect_bytes, [0x8E, 0xE3, 0x18, 0x40], "3440x1440 aspect");
        assert!(!c.is_16_9);
        assert!((c.hor_plus_90_deg - 106.69).abs() < 0.01, "90° camera -> 106.69°, got {}", c.hor_plus_90_deg);

        // 16:9 resolutions short-circuit and never produce the DANGER bytes via a write.
        assert!(compute_values(1920, 1080).is_16_9);
        assert!(compute_values(2560, 1440).is_16_9);
        assert_eq!(compute_values(1920, 1080).aspect_bytes, DANGER_UI);

        // Aspect-sharing presets compute identical bytes.
        assert_eq!(compute_values(2560, 1080).aspect_bytes, compute_values(5120, 2160).aspect_bytes);
    }

    #[test]
    fn pattern_wildcards() {
        let p = parse_pattern("AA ?? CC");
        assert_eq!(find_pattern(&[0x00, 0xAA, 0x01, 0xCC, 0xAA, 0x02, 0xCC], &p), vec![1, 4]);
        assert!(find_pattern(&[0xAA, 0x01, 0xCD], &p).is_empty());
        assert!(find_pattern(&[0xAA], &p).is_empty());
    }

    #[test]
    fn projection_routines_layout() {
        let r = projection_routines(0x1808, 0x2000);
        assert_eq!(r.len(), PROJ_ROUTINE_LEN);
        assert_eq!(r[21], 0xC3, "post_c ends with ret");
        assert_eq!(r[46], 0xC3, "post_u ends with ret");
        assert_eq!(&r[47..], &(9.0_f32 / 16.0).to_le_bytes());
        // RIP-relative references resolve to the constant and to 1.0.
        assert_eq!(0x1808 + 13 + read_i32(&r, 9).unwrap() as i64, 0x1808 + PROJ_CONST_OFFSET as i64);
        assert_eq!(0x1808 + 21 + read_i32(&r, 17).unwrap() as i64, 0x2000);
        assert_eq!(0x1808 + 22 + 13 + read_i32(&r, 31).unwrap() as i64, 0x1808 + PROJ_CONST_OFFSET as i64);
    }

    #[test]
    fn fresh_exe_plans_aspect_and_projection() {
        let b = synth_exe();
        let plan = build_plan(&b, &opt(3440, 1440));
        assert!(plan.abort_reason.is_none(), "{:?}", plan.abort_reason);
        assert!(plan.will_write);
        let aspect: Vec<_> = plan.sites.iter().filter(|s| s.kind == EditKind::Aspect).collect();
        assert_eq!(aspect.len(), 4);
        assert!(aspect.iter().all(|s| s.state == SiteState::Patch));
        let proj = plan.sites.iter().find(|s| s.kind == EditKind::Projection).unwrap();
        assert_eq!(proj.state, SiteState::Patch);
        let cave = SYN_RUN + PROJ_PADDING_LEAD;
        assert_eq!(proj.writes[0], (cave as usize, projection_routines(cave, SYN_ONE)));
        let site_c = SYN_C + PROJ_SITE_OFFSET;
        assert_eq!(proj.writes[1].0, site_c);
        assert_eq!(proj.writes[1].1[0], 0xE8);
        assert_eq!(site_c as i64 + 5 + read_i32(&proj.writes[1].1, 1).unwrap() as i64, cave as i64);
        let site_u = SYN_U + PROJ_SITE_OFFSET;
        assert_eq!(proj.writes[2].0, site_u);
        assert_eq!(proj.writes[2].1.len(), 11);
        assert_eq!(site_u as i64 + 5 + read_i32(&proj.writes[2].1, 1).unwrap() as i64, (cave + PROJ_POST_U_OFFSET) as i64);
        assert!(plan.sites.iter().all(|s| s.kind != EditKind::Restore));
    }

    #[test]
    fn apply_verifies_and_rerun_is_already() {
        let (dir, exe, backups) = temp_exe("apply", &synth_exe());
        let report = apply(&exe, &opt(3440, 1440), &backups).unwrap();
        assert!(report.ok, "apply should verify ok: {}", report.message);
        assert_eq!(report.residual_required, 0);
        assert!(report.size_unchanged);

        let after = std::fs::read(&exe).unwrap();
        let plan = build_plan(&after, &opt(3440, 1440));
        assert!(plan.abort_reason.is_none(), "{:?}", plan.abort_reason);
        assert!(plan.sites.iter().all(|s| s.state == SiteState::Already));

        let again = apply(&exe, &opt(3440, 1440), &backups).unwrap();
        assert!(again.ok);
        assert!(again.message.to_lowercase().contains("already"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_16_9_is_noop() {
        let plan = build_plan(&synth_exe(), &opt(2560, 1440));
        assert!(plan.no_change_16_9);
        assert!(!plan.will_write);
    }

    #[test]
    fn required_duplicate_aborts() {
        let mut b = synth_exe();
        // A second copy of the bare render-aspect value.
        b[0x2020..0x2024].copy_from_slice(&OLD_RENDER_169);
        assert!(build_plan(&b, &opt(3440, 1440)).abort_reason.is_some(), "duplicate aspect site must abort");
    }

    #[test]
    fn partial_aspect_build_aborts() {
        // Only the bare render aspect (group A) is left; the camera sites are gone →
        // mixed states must abort instead of writing a stretched partial patch.
        let mut b = synth_exe();
        b[0x1010..0x1100].fill(0x90);
        assert!(build_plan(&b, &opt(3440, 1440)).abort_reason.is_some(), "partial aspect build must abort");
    }

    #[test]
    fn unrecognized_projection_code_aborts() {
        let mut b = synth_exe();
        b[SYN_U + 0x40] = 0x90; // the unconstrained follow-up check no longer matches
        let plan = build_plan(&b, &opt(3440, 1440));
        assert!(plan.abort_reason.as_deref().unwrap_or("").contains("projection"), "{:?}", plan.abort_reason);

        let mut b = synth_exe();
        b[SYN_C..SYN_C + 4].fill(0x90); // constrained site gone
        assert!(build_plan(&b, &opt(3440, 1440)).abort_reason.is_some());
    }

    #[test]
    fn padding_inside_a_function_is_never_used() {
        let mut b = synth_exe();
        put_u32(&mut b, 0x3004, 0x2000); // the .pdata function now covers the int3 run
        let plan = build_plan(&b, &opt(3440, 1440));
        assert!(plan.abort_reason.as_deref().unwrap_or("").contains("padding"), "{:?}", plan.abort_reason);
    }

    #[test]
    fn outdated_patch_is_upgraded() {
        // Aspect edits already applied by v1.0.x, projection fix missing, and a v1.0 FOV edit
        // still holding its widened value.
        let mut b = synth_exe();
        let aspect = compute_values(3440, 1440).aspect_bytes;
        for s in build_plan(&b, &opt(3440, 1440)).sites.iter().filter(|s| s.kind == EditKind::Aspect) {
            let o = s.offset.unwrap() as usize;
            b[o..o + 4].copy_from_slice(&aspect);
        }
        let legacy = &LEGACY_RESTORES[0];
        let o = 0x1400;
        b[o..o + legacy.prefix.len()].copy_from_slice(legacy.prefix);
        let v = o + legacy.prefix.len();
        b[v..v + 4].copy_from_slice(&106.69_f32.to_le_bytes());
        b[v + 4..v + 4 + legacy.suffix.len()].copy_from_slice(legacy.suffix);

        let plan = build_plan(&b, &opt(3440, 1440));
        assert!(plan.abort_reason.is_none(), "{:?}", plan.abort_reason);
        assert!(plan.sites.iter().filter(|s| s.kind == EditKind::Aspect).all(|s| s.state == SiteState::Already));
        assert!(plan.sites.iter().any(|s| s.kind == EditKind::Projection && s.state == SiteState::Patch));
        assert_eq!(plan.sites.iter().filter(|s| s.kind == EditKind::Restore).count(), 1);

        let (dir, exe, backups) = temp_exe("upgrade", &b);
        assert_eq!(inspect(&exe, Store::Manual, &backups).unwrap().state, ExeState::OutdatedPatch);
        let report = apply(&exe, &opt(3440, 1440), &backups).unwrap();
        assert!(report.ok, "{}", report.message);
        let after = std::fs::read(&exe).unwrap();
        assert_eq!(after[v..v + 4], OLD_FOV_90, "legacy FOV edit restored to 90.0");
        let info = inspect(&exe, Store::Manual, &backups).unwrap();
        assert_eq!(info.state, ExeState::AlreadyPatched);
        assert_eq!(info.legacy_edits, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A legacy edit context with `value` between prefix and suffix.
    fn legacy(r: &RestoreDescriptor, value: [u8; 4]) -> Vec<u8> {
        let mut v = r.prefix.to_vec();
        v.extend_from_slice(&value);
        v.extend_from_slice(r.suffix);
        v.extend_from_slice(&[0x90u8; 8]);
        v
    }

    #[test]
    fn restore_needs_one_changed_context() {
        let wide = 106.69_f32.to_le_bytes();
        let r = &LEGACY_RESTORES[3];
        assert_eq!(plan_restores(&legacy(r, wide)).len(), 1);
        // Still the original value → nothing to restore.
        assert!(plan_restores(&legacy(r, OLD_FOV_90)).is_empty());
        // Context present twice → ambiguous → leave both alone.
        assert!(plan_restores(&[legacy(r, wide), legacy(r, wide)].concat()).is_empty());
        // Prefix without its suffix → not the edit site.
        let mut no_suffix = r.prefix.to_vec();
        no_suffix.extend_from_slice(&wide);
        no_suffix.extend_from_slice(&[0x90u8; 24]);
        assert!(plan_restores(&no_suffix).is_empty());
    }

    #[test]
    fn invalid_dimensions_abort() {
        assert!(build_plan(&[0u8; 16], &opt(99999, 1440)).abort_reason.is_some());
        assert!(build_plan(&[0u8; 16], &opt(1, 1440)).abort_reason.is_some());
        assert!(build_plan(&[0u8; 16], &opt(3840, 100)).abort_reason.is_some());
    }

    /// The clean baseline from `KH3_EXE_COPY` (e.g. the project's `_backup\*.orig`), or None to
    /// skip. Normal `cargo test` and CI stay machine-independent and PII-free.
    fn real_baseline(test: &str) -> Option<Vec<u8>> {
        let Some(src) = std::env::var_os("KH3_EXE_COPY") else {
            eprintln!("{test}: KH3_EXE_COPY not set — skipping");
            return None;
        };
        let base = std::fs::read(&src).expect("KH3_EXE_COPY unreadable");
        assert_eq!(sha256_bytes(&base), BASELINE_SHA, "KH3_EXE_COPY must be the clean baseline");
        Some(base)
    }

    /// Real-bytes golden test: patching the clean baseline at 3440x1440 must reproduce the
    /// in-game-validated build byte-for-byte, and revert must restore the baseline.
    #[test]
    fn golden_real_exe() {
        let Some(base) = real_baseline("golden_real_exe") else { return };
        let (dir, exe, backups) = temp_exe("golden", &base);
        assert_eq!(inspect(&exe, Store::Manual, &backups).unwrap().state, ExeState::CleanBaseline);

        let rep = apply(&exe, &opt(3440, 1440), &backups).unwrap();
        assert!(rep.ok, "patch verify failed: {}", rep.message);
        assert_eq!(rep.sha_after, PATCHED_3440_SHA, "patched bytes must match golden SHA");
        assert!(rep.matches_known_patched);
        assert_eq!(inspect(&exe, Store::Manual, &backups).unwrap().state, ExeState::AlreadyPatched);

        let rev = revert(&exe, &backups).unwrap();
        assert_eq!(rev.sha_after, BASELINE_SHA, "revert must restore the baseline");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v1.0.x's 3440x1440 build (FOV constant edits instead of the projection fix).
    const LEGACY_V10X_3440_SHA: &str = "1EABCFFB09AE443521B42868E02EA126E3B346D48A859DB1021642891DA2FBBC";

    /// Real-bytes: builds patched by v1.0.x (1EABCFFB) and by v1.0.0 with its camera option ticked
    /// (5CF3CF63…, seen in the field) must both upgrade to exactly the golden build.
    #[test]
    fn golden_real_exe_upgrades_older_builds() {
        let Some(base) = real_baseline("golden_real_exe_upgrades_older_builds") else { return };
        let (aspect, old_fov) = ([0x8E, 0xE3, 0x18, 0x40], [0x25, 0x60, 0xD5, 0x42]);
        let mut v10x = base.clone();
        for off in [0x65675C8usize, 0x3FA1D5B, 0x3FA3212, 0x3FA1DA8] {
            v10x[off..off + 4].copy_from_slice(&aspect);
        }
        for off in [0x3FA1D3Cusize, 0x3FA1D97, 0x3FA3208] {
            v10x[off..off + 4].copy_from_slice(&old_fov);
        }
        assert_eq!(sha256_bytes(&v10x), LEGACY_V10X_3440_SHA, "fixture must reproduce the v1.0.x build");
        let mut v100_camera_option = v10x.clone();
        for off in [0x360A604usize, 0x4028EB3] {
            v100_camera_option[off..off + 4].copy_from_slice(&old_fov);
        }
        assert!(sha256_bytes(&v100_camera_option).starts_with("5CF3CF63"), "fixture must reproduce the v1.0.0 build");

        for (tag, data, legacy_edits) in [("v10x", &v10x, 3), ("v100cam", &v100_camera_option, 5)] {
            let (dir, exe, backups) = temp_exe(tag, data);
            let info = inspect(&exe, Store::Manual, &backups).unwrap();
            assert_eq!(info.state, ExeState::OutdatedPatch, "{tag}");
            assert_eq!(info.legacy_edits, legacy_edits, "{tag}");
            let rep = apply(&exe, &opt(3440, 1440), &backups).unwrap();
            assert!(rep.ok, "{tag}: {}", rep.message);
            assert_eq!(rep.sha_after, PATCHED_3440_SHA, "{tag} must upgrade to the golden build");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
