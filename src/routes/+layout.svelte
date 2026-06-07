<script lang="ts">
	// Self-hosted fonts (bundled by Vite from node_modules → served same-origin,
	// satisfying the strict CSP and working fully offline).
	import '@fontsource/cormorant-garamond/500.css';
	import '@fontsource/cormorant-garamond/600.css';
	import '@fontsource/hanken-grotesk/400.css';
	import '@fontsource/hanken-grotesk/500.css';
	import '@fontsource/hanken-grotesk/600.css';
	import '@fontsource/hanken-grotesk/700.css';
	import '@fontsource/jetbrains-mono/400.css';
	import '@fontsource/jetbrains-mono/500.css';
	import '../app.css';

	let { children } = $props();
</script>

{@render children()}
