// Display-only helpers. The bytes actually written are always computed by the
// Rust backend from the chosen width/height — these never touch the patch.

export interface Preset {
	w: number;
	h: number;
}

export const PRESETS: Preset[] = [
	{ w: 2560, h: 1080 },
	{ w: 3440, h: 1440 },
	{ w: 3840, h: 1600 },
	{ w: 5120, h: 2160 },
	{ w: 3840, h: 1080 },
	{ w: 5120, h: 1440 }
];

/** Default preset index (3440 × 1440). */
export const DEFAULT_PRESET = 1;

export function ratioLabel(w: number, h: number): string {
	if (!w || !h) return '—';
	const r = w / h;
	// Family bands matching how these resolutions are marketed. The common "21:9"
	// ultrawides are really ~2.37–2.40 (2560×1080, 3440×1440, 3840×1600, 5120×2160).
	if (Math.abs(r - 16 / 9) < 0.03) return '16:9';
	if (r >= 2.25 && r <= 2.45) return '21:9';
	if (r >= 3.4 && r <= 3.7) return '32:9';
	if (r >= 4.7 && r <= 5.5) return '48:9';
	return `${r.toFixed(2)}:1`;
}

export function formatInt(n: number): string {
	return Math.round(n).toLocaleString('en-US');
}

export function shortSha(s: string, n = 12): string {
	return s ? `${s.slice(0, n)}…` : '';
}
