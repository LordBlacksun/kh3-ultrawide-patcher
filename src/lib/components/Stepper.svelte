<script lang="ts">
	let { steps, active }: { steps: string[]; active: number } = $props();
</script>

<nav class="stepper" aria-label="Progress">
	{#each steps as label, i}
		<div class="node" class:done={i < active} class:active={i === active}>
			<span class="dot">
				{#if i < active}
					<svg viewBox="0 0 24 24" class="chk" aria-hidden="true">
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
					<span class="num">{i + 1}</span>
				{/if}
			</span>
			<span class="lbl">{label}</span>
		</div>
		{#if i < steps.length - 1}<span class="bar" class:fill={i < active}></span>{/if}
	{/each}
</nav>

<style>
	.stepper {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
	}
	.node {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 9px;
		width: 76px;
	}
	.dot {
		width: 34px;
		height: 34px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: rgba(255, 255, 255, 0.035);
		border: 1px solid var(--line);
		color: var(--text-faint);
		font-family: var(--font-mono);
		font-size: 13px;
		transition: all 0.35s var(--ease);
	}
	.num {
		line-height: 1;
	}
	.lbl {
		font-size: 11px;
		letter-spacing: 0.09em;
		text-transform: uppercase;
		color: var(--text-faint);
		transition: color 0.35s var(--ease);
	}
	.node.done .dot {
		color: var(--gold);
		border-color: rgba(244, 206, 123, 0.45);
		background: rgba(244, 206, 123, 0.08);
	}
	.node.done .lbl {
		color: var(--text-dim);
	}
	.node.active .dot {
		color: #0b1020;
		border-color: transparent;
		background: var(--accent);
		box-shadow: var(--glow-gold);
		animation: glowPulse 2.6s var(--ease) infinite;
	}
	.node.active .lbl {
		color: var(--text);
	}
	.chk {
		width: 18px;
		height: 18px;
	}
	.bar {
		flex: 1;
		height: 2px;
		max-width: 44px;
		margin-bottom: 26px;
		border-radius: 2px;
		background: var(--line);
		transition: background 0.4s var(--ease);
	}
	.bar.fill {
		background: linear-gradient(90deg, rgba(244, 206, 123, 0.6), rgba(110, 134, 242, 0.6));
	}
</style>
