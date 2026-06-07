//! Data model: serde structs returned to the frontend, plus the byte-level patch
//! specification (edit descriptors + known constants). The patch spec is a faithful
//! port of the proven PowerShell patcher.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Frontend-facing types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Store {
    Steam,
    Epic,
    Manual,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExeState {
    /// Pristine, matches the known clean baseline.
    CleanBaseline,
    /// No required site still has its original value (already patched).
    AlreadyPatched,
    /// At least one required site still holds its original value.
    Patchable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub store: Store,
    pub exe_path: String,
    pub size: u64,
    pub sha256: String,
    pub is_baseline: bool,
    pub state: ExeState,
    pub backup_present: bool,
    pub backup_path: Option<String>,
    pub on_protected_path: bool,
    pub writable: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub candidates: Vec<GameInfo>,
    pub steam_root: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchOptions {
    pub width: u32,
    pub height: u32,
    /// None / <=0 → auto Hor+ from aspect. Positive → explicit FOV degrees.
    #[serde(default)]
    pub fov_degrees: Option<f64>,
    #[serde(default)]
    pub include_advanced: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedValues {
    pub aspect: f32,
    pub aspect_bytes: [u8; 4],
    pub aspect_hex: String,
    pub fov_deg: f64,
    pub fov_bytes: [u8; 4],
    pub fov_hex: String,
    #[serde(rename = "is16_9")]
    pub is_16_9: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SiteState {
    /// Will be / was patched.
    Patch,
    /// Old value absent — nothing to do (already patched).
    Already,
    /// Required site matched more than once — aborts the whole operation.
    Abort,
    /// Optional site not uniquely found — skipped (never aborts).
    Skipped,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditKind {
    Aspect,
    Fov,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePlan {
    pub name: String,
    pub group: String,
    pub optional: bool,
    pub kind: EditKind,
    pub offset: Option<u64>,
    pub state: SiteState,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPlan {
    pub computed: ComputedValues,
    pub sites: Vec<SitePlan>,
    pub will_write: bool,
    pub abort_reason: Option<String>,
    #[serde(rename = "noChange16_9")]
    pub no_change_16_9: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchReport {
    pub ok: bool,
    pub size_before: u64,
    pub size_after: u64,
    pub size_unchanged: bool,
    pub sha_before: String,
    pub sha_after: String,
    pub residual_required: usize,
    pub matches_known_patched: bool,
    pub applied: Vec<SitePlan>,
    pub backup_path: Option<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Patch specification (byte level)
// ---------------------------------------------------------------------------

/// One byte edit: locate `prefix ++ old` exactly once, overwrite the trailing
/// 4-byte float with the computed aspect/FOV value.
pub struct EditDescriptor {
    pub name: &'static str,
    pub group: char,
    pub optional: bool,
    pub prefix: &'static [u8],
    pub old: [u8; 4],
    pub kind: EditKind,
}

/// Render-table 16:9 (1.77770) — a bare, unique data float.
pub const OLD_RENDER_169: [u8; 4] = [0xAC, 0x8B, 0xE3, 0x3F];
/// Camera-projection 16:9 (1.777778) immediate written into FMinimalViewInfo.
pub const OLD_CAM_169: [u8; 4] = [0x3B, 0x8E, 0xE3, 0x3F];
/// Camera FOV 90.0 degrees.
pub const OLD_FOV_90: [u8; 4] = [0x00, 0x00, 0xB4, 0x42];
/// DANGER: f32(1920/1080) encodes to these exact bytes, which is ALSO the value
/// that, written via `mov [rax+0x428]`, boxes the entire UI. We must never write
/// it. (Camera search-old is `3B 8E E3 3F`, NOT this `39 8E E3 3F`.)
pub const DANGER_UI: [u8; 4] = [0x39, 0x8E, 0xE3, 0x3F];

/// The 7 always-applied edits (output aspect + 3 camera aspects + 3 camera FOVs).
pub const DEFAULT_EDITS: &[EditDescriptor] = &[
    EditDescriptor { name: "output/render aspect",      group: 'A', optional: false, prefix: &[],                                       old: OLD_RENDER_169, kind: EditKind::Aspect },
    EditDescriptor { name: "camera aspect [rax+0x428]", group: 'B', optional: false, prefix: &[0xC7, 0x80, 0x28, 0x04, 0x00, 0x00],    old: OLD_CAM_169,    kind: EditKind::Aspect },
    EditDescriptor { name: "camera aspect [rbx+0x428]", group: 'B', optional: false, prefix: &[0xC7, 0x83, 0x28, 0x04, 0x00, 0x00],    old: OLD_CAM_169,    kind: EditKind::Aspect },
    EditDescriptor { name: "camera aspect [rdi+0x408]", group: 'B', optional: false, prefix: &[0xC7, 0x87, 0x08, 0x04, 0x00, 0x00],    old: OLD_CAM_169,    kind: EditKind::Aspect },
    EditDescriptor { name: "camera FOV [rax+0x418]",    group: 'C', optional: false, prefix: &[0xC7, 0x80, 0x18, 0x04, 0x00, 0x00],    old: OLD_FOV_90,     kind: EditKind::Fov },
    EditDescriptor { name: "camera FOV [rdi+0x40C]",    group: 'C', optional: false, prefix: &[0xC7, 0x87, 0x0C, 0x04, 0x00, 0x00],    old: OLD_FOV_90,     kind: EditKind::Fov },
    EditDescriptor { name: "camera FOV [rbx+0x418]",    group: 'C', optional: false, prefix: &[0xC7, 0x83, 0x18, 0x04, 0x00, 0x00],    old: OLD_FOV_90,     kind: EditKind::Fov },
];

/// The 6 optional combat/team-attack FOV edits. Approximate reg/disp combos →
/// require exactly-one but SKIP (never abort) on 0 or >1 matches.
pub const ADVANCED_EDITS: &[EditDescriptor] = &[
    EditDescriptor { name: "combat FOV +0x404",       group: 'X', optional: true, prefix: &[0xC7, 0x80, 0x04, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
    EditDescriptor { name: "combat FOV +0x478 (rax)", group: 'X', optional: true, prefix: &[0xC7, 0x80, 0x78, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
    EditDescriptor { name: "combat FOV +0x478 (rbx)", group: 'X', optional: true, prefix: &[0xC7, 0x83, 0x78, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
    EditDescriptor { name: "combat FOV +0x478 (rdi)", group: 'X', optional: true, prefix: &[0xC7, 0x87, 0x78, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
    EditDescriptor { name: "combat FOV +0x414",       group: 'X', optional: true, prefix: &[0xC7, 0x80, 0x14, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
    EditDescriptor { name: "combat FOV +0x4BC",       group: 'X', optional: true, prefix: &[0xC7, 0x83, 0xBC, 0x04, 0x00, 0x00], old: OLD_FOV_90, kind: EditKind::Fov },
];

/// Known clean baseline (Steam build 14790811, ProductVersion 1.0.0.0).
pub const BASELINE_SIZE: u64 = 150_713_896;
pub const BASELINE_SHA: &str = "F53C398936560D543F2AA8E6283733572FDF8AD7C14E03459C12E039CB1BD0BC";
/// Golden patched build (3440x1440 Hor+) — used only as an informational check.
pub const PATCHED_3440_SHA: &str = "1EABCFFB09AE443521B42868E02EA126E3B346D48A859DB1021642891DA2FBBC";

pub const KH3_APPID: &str = "2552450";
pub const EXE_NAME: &str = "KINGDOM HEARTS III.exe";
