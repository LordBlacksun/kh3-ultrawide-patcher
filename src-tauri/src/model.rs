//! Data model: serde structs returned to the frontend, plus the byte-level patch
//! specification (edit descriptors, code patterns + known constants).

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
    /// Fully patched by this version (nothing left to write).
    AlreadyPatched,
    /// Patched by an older version (v1.0.x): patching again updates it.
    OutdatedPatch,
    /// At least one original edit site is still unpatched.
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
    /// Edits left by an older patcher version (restored by the next patch).
    pub legacy_edits: usize,
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
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedValues {
    pub aspect: f32,
    pub aspect_bytes: [u8; 4],
    pub aspect_hex: String,
    /// Horizontal angle a 90° camera renders at with the Hor+ fix (display only).
    #[serde(rename = "horPlus90Deg")]
    pub hor_plus_90_deg: f64,
    #[serde(rename = "is16_9")]
    pub is_16_9: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SiteState {
    /// Will be / was patched.
    Patch,
    /// Already in its patched form — nothing to do.
    Already,
    /// Not recognized (missing, duplicated or inconsistent) — aborts the whole operation.
    Abort,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditKind {
    Aspect,
    /// The Hor+ projection fix (code edit + routines in unused padding).
    Projection,
    /// Puts back an original value that an older patcher version changed.
    Restore,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePlan {
    pub name: String,
    pub group: String,
    pub kind: EditKind,
    pub offset: Option<u64>,
    pub state: SiteState,
    pub count: usize,
    /// The byte ranges this site writes: (file offset, bytes). Backend-only.
    #[serde(skip)]
    pub writes: Vec<(usize, Vec<u8>)>,
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

/// One aspect edit: locate `prefix ++ old` exactly once, overwrite the trailing
/// 4-byte float with the computed aspect ratio.
pub struct EditDescriptor {
    pub name: &'static str,
    pub group: char,
    pub prefix: &'static [u8],
    pub old: [u8; 4],
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

/// The 4 aspect edits (output aspect + 3 camera aspects).
pub const ASPECT_EDITS: &[EditDescriptor] = &[
    EditDescriptor { name: "output/render aspect",      group: 'A', prefix: &[],                                    old: OLD_RENDER_169 },
    EditDescriptor { name: "camera aspect [rax+0x428]", group: 'B', prefix: &[0xC7, 0x80, 0x28, 0x04, 0x00, 0x00], old: OLD_CAM_169 },
    EditDescriptor { name: "camera aspect [rbx+0x428]", group: 'B', prefix: &[0xC7, 0x83, 0x28, 0x04, 0x00, 0x00], old: OLD_CAM_169 },
    EditDescriptor { name: "camera aspect [rdi+0x408]", group: 'B', prefix: &[0xC7, 0x87, 0x08, 0x04, 0x00, 0x00], old: OLD_CAM_169 },
];

// --- Hor+ projection fix -------------------------------------------------------------------
//
// KH3's cameras take their FOV from game data (100° regular camera, 40–110° specials, cutscene
// FOVs that change every shot), so no set of FOV constants can make them all Hor+. The fix goes
// where every FOV ends up: FMinimalViewInfo::CalculateProjectionMatrixGivenView. Each perspective
// path there calls tanf(halfFOV) and builds the matrix from the result. The instruction(s) right
// after each call are replaced by `call <leaf routine>` + NOPs; the routine (in unused int3
// padding) multiplies the tangent by aspect * 9/16 and re-executes what it displaced. Every camera
// then keeps the vertical framing its FOV has at 16:9, with the extra width added at the sides.
// Patterns use `??` for bytes that vary (call/RIP-relative displacements).

/// Constrained path: divss / movaps xmm0,xmm2 / call tanf / movss xmm6,[rip->1.0] / movaps xmm1,xmm6
/// / mov qword [rbp-4Dh],0 / divss xmm1,xmm0 / mov qword [rbp-2Dh],3F800000h
pub const PROJ_CONSTRAINED_OLD: &str =
    "F3 0F 5E F8 0F 28 C2 E8 ?? ?? ?? ?? F3 0F 10 35 ?? ?? ?? ?? 0F 28 CE 48 C7 45 B3 00 00 00 00 F3 0F 5E C8 48 C7 45 D3 00 00 80 3F";
/// Constrained path after patching: the movss became `call <post_c>` + 3-byte NOP.
pub const PROJ_CONSTRAINED_NEW: &str =
    "F3 0F 5E F8 0F 28 C2 E8 ?? ?? ?? ?? E8 ?? ?? ?? ?? 0F 1F 00 0F 28 CE 48 C7 45 B3 00 00 00 00 F3 0F 5E C8 48 C7 45 D3 00 00 80 3F";
/// Unconstrained path: same, without the movss.
pub const PROJ_UNCONSTRAINED_OLD: &str =
    "F3 0F 5E F8 0F 28 C2 E8 ?? ?? ?? ?? 0F 28 CE 48 C7 45 B3 00 00 00 00 F3 0F 5E C8 48 C7 45 D3 00 00 80 3F";
/// Unconstrained path after patching: movaps + mov became `call <post_u>` + 6-byte NOP.
pub const PROJ_UNCONSTRAINED_NEW: &str =
    "F3 0F 5E F8 0F 28 C2 E8 ?? ?? ?? ?? E8 ?? ?? ?? ?? 66 0F 1F 44 00 00 F3 0F 5E C8 48 C7 45 D3 00 00 80 3F";
/// Offset of the displaced instruction(s) inside the patterns.
pub const PROJ_SITE_OFFSET: usize = 12;
/// Must follow the constrained site: `mulss xmm2,[rbx+28h]` (AspectRatio lives at ViewInfo+28h).
pub const PROJ_CHECK_CONSTRAINED: &[u8] = &[0xF3, 0x0F, 0x59, 0x53, 0x28];
/// Must follow the unconstrained site: `mulss xmm1,xmm10 ; mulss xmm2,xmm11` (axis multipliers).
pub const PROJ_CHECK_UNCONSTRAINED: &[u8] = &[0xF3, 0x41, 0x0F, 0x59, 0xCA, 0xF3, 0x41, 0x0F, 0x59, 0xD3];
pub const PROJ_CHECK_WINDOW: usize = 0x80;
/// Smallest int3 run considered for the routines, and how far into it they start.
pub const PROJ_MIN_PADDING: usize = 72;
pub const PROJ_PADDING_LEAD: u32 = 8;
/// Routine layout: post_c (22 bytes), post_u (25 bytes), then the 9/16 constant.
pub const PROJ_POST_U_OFFSET: u32 = 22;
pub const PROJ_CONST_OFFSET: u32 = 47;
pub const PROJ_ROUTINE_LEN: usize = 51;

// --- Edits left by older patcher versions ----------------------------------------------------

/// An older version's write, located by the bytes around it (the value it wrote varied with the
/// chosen resolution) and restored to `original`.
pub struct RestoreDescriptor {
    pub name: &'static str,
    pub prefix: &'static [u8],
    pub suffix: &'static [u8],
    pub original: [u8; 4],
}

pub const LEGACY_RESTORES: &[RestoreDescriptor] = &[
    // v1.0.x wrote the Hor+ FOV into these three camera constructors. The projection fix covers
    // them now; left in place they would be widened twice.
    RestoreDescriptor {
        name: "v1.0 FOV edit [rax+0x418]",
        prefix: &[0x48, 0x89, 0x87, 0xE8, 0x03, 0x00, 0x00, 0xC7, 0x80, 0x18, 0x04, 0x00, 0x00],
        suffix: &[0x48, 0x8B, 0x87, 0xE8, 0x03, 0x00, 0x00, 0x83, 0x88, 0x38, 0x04, 0x00, 0x00, 0x01],
        original: OLD_FOV_90,
    },
    RestoreDescriptor {
        name: "v1.0 FOV edit [rdi+0x40C]",
        prefix: &[0xC7, 0x87, 0x0C, 0x04, 0x00, 0x00],
        suffix: &[0x83, 0x8F, 0x04, 0x04, 0x00, 0x00, 0x01, 0xC7, 0x87, 0x08, 0x04, 0x00, 0x00],
        original: OLD_FOV_90,
    },
    RestoreDescriptor {
        name: "v1.0 FOV edit [rbx+0x418]",
        prefix: &[0x48, 0x89, 0x83, 0xE0, 0x0B, 0x00, 0x00, 0xC7, 0x83, 0x18, 0x04, 0x00, 0x00],
        suffix: &[0xC7, 0x83, 0x28, 0x04, 0x00, 0x00],
        original: OLD_FOV_90,
    },
    // v1.0.0's "Also widen combat & team-attack cameras" option. Its 90.0 sites were never
    // cameras: per the engine's own property registrations, the two that matched the real exe are
    // an ocean wave's `WindAngle` and the `SceneCaptureComponent2D` default `FOVAngle`.
    // mov rax,[rdi+1720h] / mov [rax+414h],<f> / mov rax,[rdi+1720h] / mov [rax+40Ch],1.0
    RestoreDescriptor {
        name: "v1.0.0 stray edit: ocean WindAngle [rax+0x414]",
        prefix: &[0x48, 0x8B, 0x87, 0x20, 0x17, 0x00, 0x00, 0xC7, 0x80, 0x14, 0x04, 0x00, 0x00],
        suffix: &[0x48, 0x8B, 0x87, 0x20, 0x17, 0x00, 0x00, 0xC7, 0x80, 0x0C, 0x04, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F],
        original: OLD_FOV_90,
    },
    // mov [rbx+4BCh],<f> / mov [rbx+4C0h],512.0 (OrthoWidth) / and [rbx+4C4h],0FFFFFFFEh
    RestoreDescriptor {
        name: "v1.0.0 stray edit: SceneCaptureComponent2D FOVAngle [rbx+0x4BC]",
        prefix: &[0xC7, 0x83, 0xBC, 0x04, 0x00, 0x00],
        suffix: &[0xC7, 0x83, 0xC0, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x44, 0x83, 0xA3, 0xC4, 0x04, 0x00, 0x00, 0xFE],
        original: OLD_FOV_90,
    },
];

/// Known clean baseline (Steam build 14790811, ProductVersion 1.0.0.0).
pub const BASELINE_SIZE: u64 = 150_713_896;
pub const BASELINE_SHA: &str = "F53C398936560D543F2AA8E6283733572FDF8AD7C14E03459C12E039CB1BD0BC";
/// Golden patched build (3440x1440, Hor+ projection fix) — validated in-game.
pub const PATCHED_3440_SHA: &str = "9A2582F62C1E0142AA1B416DD3D9D403D32CC2EB6DA462FCEA5F4E2163F5C96C";

pub const KH3_APPID: &str = "2552450";
pub const EXE_NAME: &str = "KINGDOM HEARTS III.exe";
