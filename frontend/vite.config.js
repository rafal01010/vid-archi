import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

function normalizeAllowedHost(value) {
	if (!value) {
		return null;
	}

	try {
		return new URL(value).hostname;
	} catch {
		return value
			.replace(/^https?:\/\//, '')
			.split('/')[0]
			.trim() || null;
	}
}

const allowedHosts = Array.from(
	new Set(
		[process.env.STG_ALB_DNS, process.env.PUBLIC_API_BASE_URL]
			.map(normalizeAllowedHost)
			.filter(Boolean)
	)
);

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		allowedHosts
	},
	preview: {
		allowedHosts
	}
});
