<script lang="ts">
	import type { GameInfo } from '$lib/types';
	import { formatInt, shortSha } from '$lib/presets';
	import StatePill from './StatePill.svelte';

	let { game, selected = false }: { game: GameInfo; selected?: boolean } = $props();

	const storeLabel = (s: string) =>
		s === 'steam' ? 'Steam' : s === 'epic' ? 'Epic Games' : s === 'manual' ? 'Manual' : 'Unknown';

	function statePill(g: GameInfo): { label: string; tone: 'ok' | 'info' | 'warn' | 'neutral' } {
		switch (g.state) {
			case 'clean_baseline':
				return { label: 'Clean baseline', tone: 'ok' };
			case 'already_patched':
				return { label: 'Already patched', tone: 'info' };
			case 'patchable':
				return { label: 'Unpatched', tone: 'neutral' };
			default:
				return { label: 'Unknown build', tone: 'warn' };
		}
	}
	let sp = $derived(statePill(game));
	let needsAdmin = $derived(game.onProtectedPath && !game.writable);
</script>

<div class="card" class:selected>
	<div class="top">
		<span class="store {game.store}">{storeLabel(game.store)}</span>
		<StatePill label={sp.label} tone={sp.tone} />
	</div>
	<p class="path mono" title={game.exePath}>{game.exePath}</p>
	<div class="meta">
		<span class="mono">{formatInt(game.size)} bytes</span>
		<span class="sep">·</span>
		<span class="mono" title={game.sha256}>SHA {shortSha(game.sha256)}</span>
	</div>
	{#if game.running || needsAdmin || game.backupPresent}
		<div class="flags">
			{#if game.running}<StatePill label="Game running" tone="warn" />{/if}
			{#if needsAdmin}<StatePill label="Needs administrator" tone="warn" />{/if}
			{#if game.backupPresent}<span class="backup">✓ Backup saved</span>{/if}
		</div>
	{/if}
</div>

<style>
	.card {
		text-align: left;
		border: 1px solid var(--line);
		background: var(--surface);
		border-radius: var(--r-md);
		padding: 16px 18px;
		box-shadow: var(--shadow-pop);
		backdrop-filter: blur(8px);
		transition:
			border-color 0.25s var(--ease),
			box-shadow 0.25s var(--ease);
	}
	.card.selected {
		border-color: var(--line-strong);
		box-shadow: var(--shadow-pop), var(--glow-gold);
	}
	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		margin-bottom: 12px;
	}
	.store {
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		padding: 3px 9px;
		border-radius: var(--r-pill);
		border: 1px solid var(--line);
		color: var(--text-dim);
	}
	.store.steam {
		color: #9bd0ff;
		border-color: rgba(120, 180, 255, 0.3);
		background: rgba(120, 180, 255, 0.08);
	}
	.store.epic {
		color: #cfd6e6;
		border-color: rgba(200, 210, 230, 0.25);
		background: rgba(200, 210, 230, 0.06);
	}
	.store.manual {
		color: var(--gold);
		border-color: rgba(244, 206, 123, 0.3);
		background: rgba(244, 206, 123, 0.07);
	}
	.path {
		font-size: 12.5px;
		color: var(--text);
		word-break: break-all;
		margin: 0 0 9px;
		line-height: 1.45;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 9px;
		font-size: 12px;
		color: var(--text-faint);
	}
	.meta .mono {
		font-size: 11.5px;
	}
	.flags {
		display: flex;
		flex-wrap: wrap;
		gap: 7px;
		margin-top: 13px;
	}
	.backup {
		font-size: 12px;
		color: var(--ok);
		align-self: center;
	}
</style>
