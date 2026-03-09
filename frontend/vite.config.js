import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

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

export default defineConfig(({ mode }) => {
	const env = {
		...loadEnv(mode, process.cwd(), ''),
		...process.env
	};
	const allowedHosts = Array.from(
		new Set(
			[
				env.STG_ALB_DNS,
				env.PUBLIC_API_BASE_URL,
				env.__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS
			]
				.flatMap((value) => String(value ?? '').split(','))
				.map(normalizeAllowedHost)
				.filter(Boolean)
		)
	);

	return {
		plugins: [sveltekit()],
		server: {
			allowedHosts
		},
		preview: {
			allowedHosts
		}
	};
});
