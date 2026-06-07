<script lang="ts">
	let { size = 28 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 48 44" fill="none" class="crown" aria-hidden="true">
	<defs>
		<linearGradient id="kh-crown" x1="4" y1="6" x2="44" y2="40" gradientUnits="userSpaceOnUse">
			<stop stop-color="#FBE3A1" />
			<stop offset=".45" stop-color="#EBB958" />
			<stop offset="1" stop-color="#C8862C" />
		</linearGradient>
	</defs>
	<path
		d="M5 13l8.5 7.5L23 5l9.5 15.5L41 13l-3.4 23.5a2 2 0 0 1-2 1.7H10.4a2 2 0 0 1-2-1.7L5 13z"
		fill="url(#kh-crown)"
		stroke="rgba(58,42,8,.55)"
		stroke-width="1.1"
		stroke-linejoin="round"
	/>
	<circle cx="5" cy="12.5" r="2.5" fill="#FBE3A1" />
	<circle cx="41" cy="12.5" r="2.5" fill="#FBE3A1" />
	<circle cx="23" cy="5" r="2.6" fill="#FBE3A1" />
</svg>

<style>
	.crown {
		display: block;
		filter: drop-shadow(0 3px 8px rgba(244, 206, 123, 0.4));
	}
</style>
