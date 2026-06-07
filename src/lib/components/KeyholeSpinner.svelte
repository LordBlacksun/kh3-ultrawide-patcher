<script lang="ts">
	let { size = 96 }: { size?: number } = $props();
</script>

<div class="kh" style="--s:{size}px" role="status" aria-label="Working">
	<svg viewBox="0 0 100 100" class="r1">
		<circle cx="50" cy="50" r="45" class="track" />
		<circle cx="50" cy="50" r="45" class="arc" />
	</svg>
	<svg viewBox="0 0 100 100" class="r2">
		<circle cx="50" cy="50" r="34" class="arc2" />
	</svg>
	<svg viewBox="0 0 100 100" class="key">
		<circle cx="50" cy="44" r="8.5" />
		<path d="M45.5 50.5h9L57 70H43z" />
	</svg>
</div>

<style>
	.kh {
		position: relative;
		width: var(--s);
		height: var(--s);
	}
	.kh svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
	}
	.track {
		fill: none;
		stroke: rgba(122, 144, 200, 0.15);
		stroke-width: 2.5;
	}
	.arc {
		fill: none;
		stroke: var(--gold);
		stroke-width: 2.5;
		stroke-linecap: round;
		stroke-dasharray: 72 220;
		filter: drop-shadow(0 0 6px rgba(244, 206, 123, 0.7));
		transform-origin: 50% 50%;
		animation: spinCW 1.5s linear infinite;
	}
	.r2 .arc2 {
		fill: none;
		stroke: var(--blue);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-dasharray: 30 180;
		opacity: 0.85;
		filter: drop-shadow(0 0 5px rgba(110, 134, 242, 0.6));
		transform-origin: 50% 50%;
		animation: spinCCW 2.4s linear infinite;
	}
	.key :global(*) {
		fill: var(--gold);
		filter: drop-shadow(0 0 7px rgba(244, 206, 123, 0.6));
		animation: glowPulse 1.8s var(--ease) infinite;
	}
</style>
