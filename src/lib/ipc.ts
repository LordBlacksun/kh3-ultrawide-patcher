// Thin wrappers over the Tauri command surface. (Tauri 2 import paths.)

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type {
	ComputedValues,
	DetectResult,
	GameInfo,
	PatchOptions,
	PatchPlan,
	PatchReport
} from './types';

export const detect = () => invoke<DetectResult>('detect');

export const inspectPath = (path: string) => invoke<GameInfo>('inspect_path', { path });

export const compute = (width: number, height: number, fov: number | null) =>
	invoke<ComputedValues>('compute', { width, height, fov });

export const planPatch = (path: string, options: PatchOptions) =>
	invoke<PatchPlan>('plan', { path, options });

export const applyPatch = (path: string, options: PatchOptions) =>
	invoke<PatchReport>('patch', { path, options });

export const revertPatch = (path: string) => invoke<PatchReport>('revert', { path });

export const isRunning = (path: string) => invoke<boolean>('is_running', { path });

/** Open the native file picker; returns the chosen exe path or null. */
export async function browseForExe(): Promise<string | null> {
	const selection = await open({
		multiple: false,
		directory: false,
		title: 'Select KINGDOM HEARTS III.exe',
		filters: [{ name: 'KH3 executable', extensions: ['exe'] }]
	});
	return typeof selection === 'string' ? selection : null;
}
