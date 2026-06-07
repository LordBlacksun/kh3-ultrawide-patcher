<script lang="ts">
	let {
		label,
		tone = 'neutral',
		dot = true
	}: { label: string; tone?: 'ok' | 'info' | 'warn' | 'err' | 'neutral'; dot?: boolean } = $props();
</script>

<span class="pill {tone}">
	{#if dot}<span class="d"></span>{/if}{label}
</span>

<style>
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 4px 11px;
		border-radius: var(--r-pill);
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.02em;
		border: 1px solid var(--line);
		background: rgba(255, 255, 255, 0.03);
		color: var(--text-dim);
		white-space: nowrap;
	}
	.d {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: currentColor;
		box-shadow: 0 0 8px currentColor;
	}
	.ok {
		color: var(--ok);
		border-color: rgba(95, 214, 166, 0.32);
		background: rgba(95, 214, 166, 0.1);
	}
	.info {
		color: var(--info);
		border-color: rgba(121, 172, 255, 0.32);
		background: rgba(121, 172, 255, 0.1);
	}
	.warn {
		color: var(--warn);
		border-color: rgba(242, 188, 99, 0.32);
		background: rgba(242, 188, 99, 0.1);
	}
	.err {
		color: var(--err);
		border-color: rgba(255, 126, 132, 0.34);
		background: rgba(255, 126, 132, 0.1);
	}
	.neutral {
		color: var(--text-dim);
	}
</style>
