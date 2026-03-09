import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

const STATIC_ALLOWED_HOSTS = ['stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com'];

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
				...STATIC_ALLOWED_HOSTS,
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
