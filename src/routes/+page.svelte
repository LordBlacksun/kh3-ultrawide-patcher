<script lang="ts">
	import { onMount } from 'svelte';
	import * as ipc from '$lib/ipc';
	import type {
		AppError,
		ComputedValues,
		DetectResult,
		GameInfo,
		PatchOptions,
		PatchPlan,
		PatchReport
	} from '$lib/types';
	import { DEFAULT_PRESET, PRESETS, ratioLabel } from '$lib/presets';
	import Crown from '$lib/components/Crown.svelte';
	import Stepper from '$lib/components/Stepper.svelte';
	import GameCard from '$lib/components/GameCard.svelte';
	import KeyholeSpinner from '$lib/components/KeyholeSpinner.svelte';

	type Phase = 'detect' | 'options' | 'working' | 'result';
	const STEPS = ['Detect', 'Configure', 'Patch', 'Done'];

	let phase = $state<Phase>('detect');
	let detecting = $state(true);
	let detectResult = $state<DetectResult | null>(null);
	let game = $state<GameInfo | null>(null);

	// resolution
	let resMode = $state<'preset' | 'auto' | 'custom'>('preset');
	let presetIdx = $state(DEFAULT_PRESET);
	let customW = $state(3440);
	let customH = $state(1440);
	let autoW = $state(3440);
	let autoH = $state(1440);

	let width = $derived(
		resMode === 'custom'
			? Math.max(1, Math.floor(Number(customW) || 0))
			: resMode === 'auto'
				? autoW
				: PRESETS[presetIdx].w
	);
	let height = $derived(
		resMode === 'custom'
			? Math.max(1, Math.floor(Number(customH) || 0))
			: resMode === 'auto'
				? autoH
				: PRESETS[presetIdx].h
	);

	// fov + flags
	let fovMode = $state<'auto' | 'custom'>('auto');
	let fovCustom = $state(106);
	let advanced = $state(false);
	let force = $state(false);

	// computed / plan
	let computed = $state<ComputedValues | null>(null);
	let plan = $state<PatchPlan | null>(null);
	let planning = $state(false);

	// outcome
	let report = $state<PatchReport | null>(null);
	let error = $state<AppError | null>(null);
	let busy = $state(false);
	let busyVerb = $state('Patching');
	let busySub = $state('Backing up and writing the ultrawide edits.');

	let candidates = $derived(detectResult?.candidates ?? []);
	let notes = $derived(detectResult?.notes ?? []);
	let activeStep = $derived(
		phase === 'detect' ? 0 : phase === 'options' ? 1 : phase === 'working' ? 2 : 3
	);
	let is16_9 = $derived(computed?.is16_9 ?? false);
	let planToPatch = $derived(plan ? plan.sites.filter((s) => s.state === 'patch').length : 0);
	let planAlready = $derived(plan ? plan.sites.filter((s) => s.state === 'already').length : 0);
	let planSkipped = $derived(plan ? plan.sites.filter((s) => s.state === 'skipped').length : 0);
	let customAspect = $derived(height > 0 ? width / height : 0);
	let dimsValid = $derived(
		width >= 1024 &&
			width <= 16384 &&
			height >= 600 &&
			height <= 8640 &&
			customAspect >= 1.6 &&
			customAspect <= 4.0
	);
	let canPatch = $derived(
		!!game && !is16_9 && dimsValid && !plan?.abortReason && (plan?.willWrite ?? true)
	);

	function opts(): PatchOptions {
		return {
			width,
			height,
			fovDegrees: fovMode === 'custom' ? fovCustom : null,
			includeAdvanced: advanced,
			force
		};
	}

	const KNOWN_RES: [number, number][] = [
		[2560, 1080],
		[3440, 1440],
		[3840, 1600],
		[5120, 2160],
		[3840, 1080],
		[5120, 1440],
		[1920, 1080],
		[2560, 1440],
		[3840, 2160],
		[1920, 1200],
		[2560, 1600],
		[1280, 720]
	];
	function snapRes(w: number, h: number): [number, number] {
		for (const [kw, kh] of KNOWN_RES) {
			if (Math.abs(w - kw) <= 4 && Math.abs(h - kh) <= 4) return [kw, kh];
		}
		return [w, h];
	}
	function updateDisplay() {
		try {
			const w = Math.round((window.screen?.width ?? 3440) * (window.devicePixelRatio || 1));
			const h = Math.round((window.screen?.height ?? 1440) * (window.devicePixelRatio || 1));
			[autoW, autoH] = snapRes(w, h);
		} catch {
			/* keep defaults */
		}
	}

	onMount(() => {
		updateDisplay();
		window.addEventListener('resize', updateDisplay);
		runDetect();
		return () => window.removeEventListener('resize', updateDisplay);
	});

	async function runDetect() {
		detecting = true;
		error = null;
		try {
			const r = await ipc.detect();
			detectResult = r;
			game = r.candidates[0] ?? null;
		} catch (e) {
			error = e as AppError;
		} finally {
			detecting = false;
		}
	}

	async function browse() {
		try {
			const p = await ipc.browseForExe();
			if (!p) return;
			const gi = await ipc.inspectPath(p);
			game = gi;
			const rest = detectResult?.candidates ?? [];
			const merged = [gi, ...rest].filter(
				(g, i, a) => a.findIndex((x) => x.exePath.toLowerCase() === g.exePath.toLowerCase()) === i
			);
			detectResult = {
				candidates: merged,
				steamRoot: detectResult?.steamRoot ?? null,
				notes: detectResult?.notes ?? []
			};
		} catch (e) {
			error = e as AppError;
		}
	}

	function selectGame(g: GameInfo) {
		game = g;
	}

	// Live aspect/FOV preview (cheap, no file I/O).
	$effect(() => {
		const w = width;
		const h = height;
		const fov = fovMode === 'custom' ? fovCustom : null;
		if (phase !== 'options') return;
		const gen = ++computeGen;
		ipc
			.compute(w, h, fov)
			.then((c) => {
				if (gen === computeGen) computed = c;
			})
			.catch(() => {});
	});

	// Plan (reads the exe) — only depends on the install, resolution's 16:9-ness,
	// and the advanced toggle. Debounced so it doesn't re-read on every keystroke.
	let planTimer: ReturnType<typeof setTimeout> | null = null;
	let planGen = 0;
	let computeGen = 0;
	$effect(() => {
		const g = game;
		const w = width;
		const h = height;
		const adv = advanced;
		if (phase !== 'options' || !g) return;
		if (planTimer) clearTimeout(planTimer);
		planning = true;
		const gen = ++planGen;
		planTimer = setTimeout(async () => {
			try {
				const p = await ipc.planPatch(g.exePath, {
					width: w,
					height: h,
					fovDegrees: null,
					includeAdvanced: adv,
					force: true
				});
				if (gen === planGen) plan = p;
			} catch {
				/* keep last plan */
			} finally {
				if (gen === planGen) planning = false;
			}
		}, 250);
		return () => {
			if (planTimer) clearTimeout(planTimer);
		};
	});

	async function doPatch() {
		if (!game) return;
		busy = true;
		busyVerb = 'Patching';
		busySub = 'Backing up and writing the ultrawide edits.';
		phase = 'working';
		error = null;
		report = null;
		try {
			report = await ipc.applyPatch(game.exePath, opts());
			phase = 'result';
			force = false;
			try {
				game = await ipc.inspectPath(game.exePath);
			} catch {
				/* ignore refresh failure */
			}
		} catch (e) {
			error = e as AppError;
			phase = 'options';
		} finally {
			busy = false;
		}
	}

	function forceRetry() {
		force = true;
		doPatch();
	}

	async function doRevert() {
		if (!game) return;
		busy = true;
		busyVerb = 'Reverting';
		busySub = 'Restoring the original executable from backup.';
		phase = 'working';
		error = null;
		report = null;
		try {
			report = await ipc.revertPatch(game.exePath);
			phase = 'result';
			try {
				game = await ipc.inspectPath(game.exePath);
			} catch {
				/* ignore */
			}
		} catch (e) {
			error = e as AppError;
			phase = 'result';
		} finally {
			busy = false;
		}
	}

	function restart() {
		report = null;
		error = null;
		phase = 'detect';
		runDetect();
	}
</script>

<div class="app-root">
	<header class="hd">
		<div class="brand">
			<Crown size={32} />
			<div class="wordmark">
				<h1>KH3 Ultrawide</h1>
				<span class="sub">Patcher</span>
			</div>
		</div>
		<p class="tag">True 21:9 &amp; 32:9 for<br />KINGDOM&nbsp;HEARTS&nbsp;III</p>
	</header>

	<Stepper steps={STEPS} active={activeStep} />

	<main class="stage">
		{#if phase === 'detect'}
			<section class="panel">
				{#if detecting}
					<div class="center tall">
						<KeyholeSpinner />
						<p class="muted big">Searching for KINGDOM HEARTS III…</p>
						<p class="muted small">Checking Steam and Epic libraries</p>
					</div>
				{:else}
					{#if candidates.length}
						<h2 class="step-h">{candidates.length > 1 ? 'Choose your install' : 'Found your game'}</h2>
						<div class="cards">
							{#each candidates as c (c.exePath)}
								{#if candidates.length > 1}
									<button
										class="pick"
										onclick={() => selectGame(c)}
										aria-pressed={game?.exePath === c.exePath}
									>
										<GameCard game={c} selected={game?.exePath === c.exePath} />
									</button>
								{:else}
									<GameCard game={c} selected />
								{/if}
							{/each}
						</div>
					{:else}
						<div class="center">
							<p class="muted big">No KINGDOM HEARTS III install detected.</p>
							<p class="muted">Browse to the game's executable to continue.</p>
						</div>
					{/if}

					{#if notes.length}
						<ul class="notes">
							{#each notes as n}<li>{n}</li>{/each}
						</ul>
					{/if}

					<div class="bar">
						<button class="btn ghost" onclick={browse}>Browse manually…</button>
						<button class="btn primary" disabled={!game} onclick={() => (phase = 'options')}>
							Configure ultrawide →
						</button>
					</div>
				{/if}
			</section>
		{/if}

		{#if phase === 'options'}
			<section class="panel">
				<h2 class="step-h">Configure</h2>

				{#if error}
					<div class="err-banner" role="alert">
						<span>{error.message}</span>
						{#if error.kind === 'locked'}
							<button onclick={forceRetry}>Force &amp; retry</button>
						{/if}
					</div>
				{/if}

				<div class="group">
					<div class="group-h">
						<span>Resolution</span>
						<span class="ratio">{ratioLabel(width, height)} · {width} × {height}</span>
					</div>
					<div class="seg" role="group" aria-label="Resolution source">
						<button class:on={resMode === 'preset'} aria-pressed={resMode === 'preset'} onclick={() => (resMode = 'preset')}>Presets</button>
						<button class:on={resMode === 'auto'} aria-pressed={resMode === 'auto'} onclick={() => (resMode = 'auto')}>My display</button>
						<button class:on={resMode === 'custom'} aria-pressed={resMode === 'custom'} onclick={() => (resMode = 'custom')}>Custom</button>
					</div>

					{#if resMode === 'preset'}
						<div class="presets">
							{#each PRESETS as p, i}
								<button
									class="preset"
									class:on={presetIdx === i}
									aria-pressed={presetIdx === i}
									aria-label={`${p.w} by ${p.h}, ${ratioLabel(p.w, p.h)}`}
									onclick={() => (presetIdx = i)}
								>
									<span class="pv">{p.w} × {p.h}</span>
									<span class="pr">{ratioLabel(p.w, p.h)}</span>
								</button>
							{/each}
						</div>
					{:else if resMode === 'auto'}
						<div class="auto">
							Detected display: <b>{autoW} × {autoH}</b>
							<span class="ratio">{ratioLabel(autoW, autoH)}</span>
						</div>
					{:else}
						<div class="custom">
							<label>Width<input type="number" min="640" max="16000" bind:value={customW} /></label>
							<span class="x">×</span>
							<label>Height<input type="number" min="480" max="10000" bind:value={customH} /></label>
						</div>
					{/if}
				</div>

				{#if is16_9}
					<div class="warn-banner">This is a 16:9 resolution — no ultrawide change is needed.</div>
				{:else if !dimsValid}
					<div class="warn-banner">
						Enter a resolution within 1024–16384 × 600–8640 and a 16:10–32:9 aspect.
					</div>
				{/if}

				<div class="group">
					<div class="group-h"><span>Field of view</span></div>
					<div class="seg" role="group" aria-label="Field of view mode">
						<button class:on={fovMode === 'auto'} aria-pressed={fovMode === 'auto'} onclick={() => (fovMode = 'auto')}>
							Hor+ (recommended)
						</button>
						<button class:on={fovMode === 'custom'} aria-pressed={fovMode === 'custom'} onclick={() => (fovMode = 'custom')}>Custom</button>
					</div>
					{#if fovMode === 'custom'}
						<div class="slider">
							<input
								type="range"
								min="70"
								max="130"
								step="0.5"
								aria-label="Field of view in degrees"
								bind:value={fovCustom}
							/>
							<span class="sv mono">{Number(fovCustom).toFixed(1)}°</span>
						</div>
					{/if}
					{#if computed}
						<div class="readout">
							<div>
								<span class="rk">Aspect</span>
								<span class="rv mono">{computed.aspect.toFixed(4)}</span>
								<span class="hexchip mono">{computed.aspectHex}</span>
							</div>
							<div>
								<span class="rk">FOV</span>
								<span class="rv mono">
									{(fovMode === 'custom' ? Number(fovCustom) : computed.fovDeg).toFixed(2)}°
								</span>
								<span class="hexchip mono">{fovMode === 'custom' ? 'custom' : computed.fovHex}</span>
							</div>
						</div>
					{/if}
				</div>

				<label class="adv">
					<input type="checkbox" bind:checked={advanced} />
					<span>
						<b>Also widen combat &amp; team-attack cameras</b><br />
						<span class="muted small">
							Experimental — reduces extra zoom during link/team attacks and unlocked-camera
							moments. Off by default; some shots may be intentionally tight.
						</span>
					</span>
				</label>

				<div class="plan">
					{#if planning}
						<span class="muted small">Reading the executable…</span>
					{:else if plan?.abortReason}
						<span class="err-text">{plan.abortReason}</span>
					{:else if plan}
						<span class="muted small">
							{#if planToPatch > 0}<b class="gold">{planToPatch}</b> site{planToPatch === 1
									? ''
									: 's'} to patch{/if}
							{#if planAlready > 0} · {planAlready} already done{/if}
							{#if planSkipped > 0} · {planSkipped} skipped{/if}
						</span>
					{/if}
				</div>

				<div class="bar">
					<button class="btn ghost" onclick={() => (phase = 'detect')}>← Back</button>
					<button class="btn primary" disabled={!canPatch || busy} onclick={doPatch}>
						{is16_9
							? 'Pick an ultrawide resolution'
							: !dimsValid
								? 'Enter a valid resolution'
								: 'Apply ultrawide patch'}
					</button>
				</div>
			</section>
		{/if}

		{#if phase === 'working'}
			<section class="panel center tall">
				<KeyholeSpinner size={108} />
				<p class="work-h">{busyVerb}…</p>
				<p class="muted">{busySub}</p>
			</section>
		{/if}

		{#if phase === 'result'}
			<section class="panel">
				{#if report}
					<div class="result-head">
						<div class="badge {report.ok ? 'ok' : 'warn'}">
							{#if report.ok}
								<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
									<path
										d="M5 12.5l4.5 4.5L19 6"
										fill="none"
										stroke="currentColor"
										stroke-width="2.6"
										stroke-linecap="round"
										stroke-linejoin="round"
									/>
								</svg>
							{:else}
								!
							{/if}
						</div>
						<div>
							<h2 class="step-h tight">{report.ok ? 'Done' : 'Heads up'}</h2>
							<p class="muted">{report.message}</p>
						</div>
					</div>

					{#if report.applied?.length}
						<div class="verify">
							<div class="vrow">
								<span>File size unchanged</span>
								<span class={report.sizeUnchanged ? 'yes' : 'no'}>{report.sizeUnchanged ? '✓' : '✗'}</span>
							</div>
							<div class="vrow">
								<span>Original patterns remaining</span>
								<span class={report.residualRequired === 0 ? 'yes' : 'no'}>{report.residualRequired}</span>
							</div>
							{#if report.matchesKnownPatched}
								<div class="vrow">
									<span>Matches the known-good patched build</span>
									<span class="yes">✓ verified</span>
								</div>
							{/if}
							<div class="vrow col">
								<span>SHA-256</span>
								<span class="mono tiny"
									>{report.shaBefore.slice(0, 20)}… → {report.shaAfter.slice(0, 20)}…</span
								>
							</div>
						</div>
					{/if}

					{#if report.backupPath}
						<p class="muted small backup-line">Backup saved · <span class="mono tiny">{report.backupPath}</span></p>
					{/if}

					{#if report.ok && report.applied.length}
						<div class="reminder">
							<b>In-game:</b> set <b>Borderless Fullscreen</b> at {width} × {height}. Re-run the patcher
							after any Steam/Epic update or “Verify integrity of game files” — those restore the
							original executable.
						</div>
					{/if}
				{/if}

				{#if error}
					<div class="err-banner" role="alert"><span>{error.message}</span></div>
				{/if}

				<div class="bar">
					<button class="btn ghost" onclick={doRevert} disabled={busy}>Revert to original</button>
					<button class="btn primary" onclick={restart}>Patch another · Done</button>
				</div>
			</section>
		{/if}
	</main>

</div>

<style>
	.app-root {
		max-width: 660px;
		margin: 0 auto;
		padding: 30px 28px 38px;
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		gap: 26px;
	}

	/* header */
	.hd {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 13px;
	}
	.wordmark {
		display: flex;
		flex-direction: column;
		line-height: 1;
	}
	.wordmark h1 {
		font-family: var(--font-display);
		font-weight: 600;
		font-size: 31px;
		margin: 0;
		letter-spacing: 0.01em;
		background: var(--accent);
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
	}
	.wordmark .sub {
		font-family: var(--font-display);
		font-style: italic;
		font-size: 17px;
		color: var(--text-faint);
		margin-top: 1px;
	}
	.tag {
		font-size: 11.5px;
		color: var(--text-faint);
		text-align: right;
		line-height: 1.45;
		margin: 0;
		letter-spacing: 0.03em;
	}

	.stage {
		flex: 1;
	}
	.panel {
		animation: fadeUp 0.45s var(--ease) both;
	}

	.center {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 16px;
		text-align: center;
		padding: 24px 0;
	}
	.center.tall {
		min-height: 320px;
	}

	.step-h {
		font-family: var(--font-display);
		font-weight: 600;
		font-size: 27px;
		margin: 0 0 18px;
		color: var(--text);
	}
	.step-h.tight {
		margin: 0 0 5px;
	}

	.cards {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.pick {
		all: unset;
		cursor: pointer;
		display: block;
		border-radius: var(--r-md);
	}
	.pick:focus-visible {
		outline: 2px solid var(--gold);
		outline-offset: 2px;
	}

	.notes {
		list-style: none;
		padding: 0;
		margin: 16px 0 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.notes li {
		font-size: 12px;
		color: var(--text-faint);
		padding-left: 14px;
		position: relative;
		line-height: 1.45;
	}
	.notes li::before {
		content: '✦';
		position: absolute;
		left: 0;
		color: var(--gold-deep);
		font-size: 9px;
		top: 3px;
	}

	/* groups */
	.group {
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		padding: 16px 17px;
		background: var(--surface);
		margin-bottom: 14px;
		backdrop-filter: blur(6px);
	}
	.group-h {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 13px;
		font-size: 13px;
		font-weight: 600;
		letter-spacing: 0.04em;
		color: var(--text);
	}
	.ratio {
		font-size: 11.5px;
		font-weight: 500;
		color: var(--gold);
		border: 1px solid rgba(244, 206, 123, 0.25);
		background: rgba(244, 206, 123, 0.07);
		padding: 3px 10px;
		border-radius: var(--r-pill);
		font-family: var(--font-mono);
	}

	/* segmented control */
	.seg {
		display: inline-flex;
		gap: 4px;
		padding: 4px;
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		background: rgba(0, 0, 0, 0.22);
	}
	.seg button {
		border: none;
		background: transparent;
		color: var(--text-dim);
		font-size: 13px;
		font-weight: 500;
		padding: 7px 13px;
		border-radius: 9px;
		cursor: pointer;
		transition: all 0.2s var(--ease);
	}
	.seg button.on {
		background: rgba(244, 206, 123, 0.12);
		color: var(--gold);
		box-shadow: inset 0 0 0 1px rgba(244, 206, 123, 0.25);
	}
	.seg button:hover:not(.on) {
		color: var(--text);
	}

	/* presets */
	.presets {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 9px;
		margin-top: 13px;
	}
	.preset {
		display: flex;
		flex-direction: column;
		gap: 3px;
		align-items: flex-start;
		padding: 11px 13px;
		border: 1px solid var(--line);
		border-radius: var(--r-sm);
		background: rgba(255, 255, 255, 0.02);
		cursor: pointer;
		transition: all 0.2s var(--ease);
	}
	.preset:hover {
		border-color: var(--line-strong);
	}
	.preset.on {
		border-color: rgba(244, 206, 123, 0.45);
		background: rgba(244, 206, 123, 0.07);
		box-shadow: var(--glow-gold);
	}
	.preset .pv {
		font-family: var(--font-mono);
		font-size: 13px;
		color: var(--text);
	}
	.preset .pr {
		font-size: 11px;
		color: var(--text-faint);
		letter-spacing: 0.05em;
	}
	.preset.on .pr {
		color: var(--gold-deep);
	}

	.auto {
		margin-top: 13px;
		font-size: 13.5px;
		color: var(--text-dim);
		display: flex;
		align-items: center;
		gap: 11px;
	}
	.auto b {
		color: var(--text);
		font-family: var(--font-mono);
		font-weight: 500;
	}

	.custom {
		display: flex;
		align-items: flex-end;
		gap: 12px;
		margin-top: 13px;
	}
	.custom label {
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-faint);
	}
	.custom .x {
		color: var(--text-faint);
		padding-bottom: 9px;
	}
	input[type='number'] {
		width: 112px;
		background: rgba(0, 0, 0, 0.25);
		border: 1px solid var(--line);
		color: var(--text);
		border-radius: 8px;
		padding: 8px 10px;
		font-family: var(--font-mono);
		font-size: 14px;
	}
	input[type='number']:focus {
		outline: none;
		border-color: var(--line-strong);
	}

	.slider {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-top: 14px;
	}
	.slider input[type='range'] {
		flex: 1;
		accent-color: var(--gold);
	}
	.sv {
		font-size: 14px;
		color: var(--gold);
		min-width: 58px;
		text-align: right;
	}

	.readout {
		display: flex;
		gap: 26px;
		margin-top: 14px;
		flex-wrap: wrap;
	}
	.readout > div {
		display: flex;
		align-items: center;
		gap: 9px;
	}
	.rk {
		font-size: 10.5px;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-faint);
	}
	.rv {
		font-size: 14px;
		color: var(--text);
	}
	.hexchip {
		font-size: 11.5px;
		color: var(--gold);
		background: rgba(244, 206, 123, 0.08);
		border: 1px solid rgba(244, 206, 123, 0.2);
		padding: 2px 8px;
		border-radius: 6px;
		letter-spacing: 0.06em;
	}

	.adv {
		display: flex;
		gap: 12px;
		align-items: flex-start;
		padding: 14px 16px;
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		background: rgba(255, 255, 255, 0.02);
		cursor: pointer;
	}
	.adv input {
		margin-top: 3px;
		accent-color: var(--gold);
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}
	.adv b {
		font-weight: 600;
		font-size: 13.5px;
	}

	.plan {
		min-height: 20px;
		margin: 12px 2px 0;
	}
	.gold {
		color: var(--gold);
	}
	.err-text {
		color: var(--err);
		font-size: 12.5px;
		line-height: 1.5;
	}

	.small {
		font-size: 12px;
	}
	.muted {
		color: var(--text-dim);
	}
	.muted.big {
		font-size: 16px;
	}

	.warn-banner {
		border: 1px solid rgba(242, 188, 99, 0.3);
		background: rgba(242, 188, 99, 0.09);
		color: var(--warn);
		padding: 11px 14px;
		border-radius: var(--r-sm);
		font-size: 13px;
		margin-bottom: 14px;
	}
	.err-banner {
		border: 1px solid rgba(255, 126, 132, 0.3);
		background: rgba(255, 126, 132, 0.09);
		color: var(--err);
		padding: 11px 14px;
		border-radius: var(--r-sm);
		font-size: 13px;
		margin-bottom: 14px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		line-height: 1.5;
	}
	.err-banner button {
		border: 1px solid rgba(255, 126, 132, 0.4);
		background: transparent;
		color: var(--err);
		padding: 6px 12px;
		border-radius: 8px;
		cursor: pointer;
		font-size: 12.5px;
		white-space: nowrap;
	}
	.err-banner button:hover {
		background: rgba(255, 126, 132, 0.12);
	}

	/* result */
	.result-head {
		display: flex;
		gap: 15px;
		align-items: center;
		margin-bottom: 18px;
	}
	.badge {
		width: 46px;
		height: 46px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		flex-shrink: 0;
		font-size: 22px;
		font-weight: 700;
	}
	.badge.ok {
		background: rgba(95, 214, 166, 0.14);
		color: var(--ok);
		border: 1px solid rgba(95, 214, 166, 0.4);
		box-shadow: 0 0 24px -6px rgba(95, 214, 166, 0.5);
	}
	.badge.warn {
		background: rgba(242, 188, 99, 0.14);
		color: var(--warn);
		border: 1px solid rgba(242, 188, 99, 0.4);
	}
	.verify {
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		background: var(--surface);
		padding: 4px 16px;
		margin-bottom: 13px;
	}
	.vrow {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 0;
		font-size: 13px;
		color: var(--text-dim);
		border-bottom: 1px solid var(--line);
	}
	.vrow:last-child {
		border-bottom: none;
	}
	.vrow.col {
		flex-direction: column;
		align-items: flex-start;
		gap: 5px;
	}
	.yes {
		color: var(--ok);
		font-weight: 600;
	}
	.no {
		color: var(--err);
		font-weight: 600;
	}
	.tiny {
		font-size: 11px;
		color: var(--text-faint);
	}
	.backup-line {
		margin: 2px 0 14px;
		word-break: break-all;
	}
	.reminder {
		border: 1px solid rgba(110, 134, 242, 0.25);
		background: rgba(110, 134, 242, 0.08);
		border-radius: var(--r-sm);
		padding: 12px 15px;
		font-size: 12.5px;
		color: var(--text-dim);
		line-height: 1.55;
	}
	.reminder b {
		color: var(--text);
	}

	.work-h {
		font-family: var(--font-display);
		font-size: 25px;
		margin: 8px 0 0;
		color: var(--text);
	}

	/* action bar */
	.bar {
		display: flex;
		gap: 12px;
		justify-content: flex-end;
		align-items: center;
		margin-top: 24px;
	}
	.bar .ghost {
		margin-right: auto;
	}
	.btn {
		font-size: 14px;
		font-weight: 600;
		padding: 12px 20px;
		border-radius: var(--r-md);
		border: 1px solid var(--line);
		background: rgba(255, 255, 255, 0.04);
		color: var(--text);
		cursor: pointer;
		transition: all 0.2s var(--ease);
	}
	.btn:hover:not(:disabled) {
		border-color: var(--line-strong);
		background: rgba(255, 255, 255, 0.07);
	}
	.btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.btn.primary {
		position: relative;
		overflow: hidden;
		border: none;
		color: #0a0f1e;
		font-weight: 700;
		background: var(--accent);
		box-shadow: var(--glow-gold);
	}
	.btn.primary:hover:not(:disabled) {
		box-shadow: 0 0 40px -6px rgba(244, 206, 123, 0.65);
		transform: translateY(-1px);
	}
	.btn.primary:disabled {
		box-shadow: none;
		filter: grayscale(0.4);
	}
	.btn.primary::after {
		content: '';
		position: absolute;
		inset: 0;
		background: linear-gradient(100deg, transparent 30%, rgba(255, 255, 255, 0.45) 50%, transparent 70%);
		background-size: 240% 100%;
		animation: sheen 3.6s var(--ease) infinite;
		pointer-events: none;
	}
	.btn.primary:disabled::after {
		display: none;
	}

</style>
