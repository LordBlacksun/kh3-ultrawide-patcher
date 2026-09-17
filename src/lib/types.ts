// Mirrors the serde structs returned by the Rust backend (see src-tauri/src/model.rs).

export type Store = 'steam' | 'epic' | 'manual' | 'unknown';
export type ExeState = 'clean_baseline' | 'already_patched' | 'outdated_patch' | 'patchable';
export type SiteState = 'patch' | 'already' | 'abort';
export type EditKind = 'aspect' | 'projection' | 'restore';

export interface GameInfo {
	store: Store;
	exePath: string;
	size: number;
	sha256: string;
	isBaseline: boolean;
	state: ExeState;
	/** Edits left by an older patcher version (restored by the next patch). */
	legacyEdits: number;
	backupPresent: boolean;
	backupPath: string | null;
	onProtectedPath: boolean;
	writable: boolean;
	running: boolean;
}

export interface DetectResult {
	candidates: GameInfo[];
	steamRoot: string | null;
	notes: string[];
}

export interface ComputedValues {
	aspect: number;
	aspectBytes: number[];
	aspectHex: string;
	/** Horizontal angle a 90° camera renders at with the Hor+ fix. */
	horPlus90Deg: number;
	is16_9: boolean;
}

export interface SitePlan {
	name: string;
	group: string;
	kind: EditKind;
	offset: number | null;
	state: SiteState;
	count: number;
}

export interface PatchPlan {
	computed: ComputedValues;
	sites: SitePlan[];
	willWrite: boolean;
	abortReason: string | null;
	noChange16_9: boolean;
}

export interface PatchReport {
	ok: boolean;
	sizeBefore: number;
	sizeAfter: number;
	sizeUnchanged: boolean;
	shaBefore: string;
	shaAfter: string;
	residualRequired: number;
	matchesKnownPatched: boolean;
	applied: SitePlan[];
	backupPath: string | null;
	message: string;
}

export interface PatchOptions {
	width: number;
	height: number;
	force: boolean;
}

/** Serialized form of the Rust `AppError` enum. */
export interface AppError {
	kind: string;
	message: string;
}
