export const CHANNELS = ['arch', 'crates.io', 'debian', 'fedora', 'homebrew', 'npm', 'pypi'] as const;
export const SEARCH_PAGE_SIZE = 30;
export const SEARCH_PAGE_SIBLINGS = 2;
export const SEARCH_RELATED_NAME_LIMIT = 3;
export const BROWSER_DATASET_SCHEMA_VERSION = 2;

export const REPOSITORY_URL = 'https://github.com/NightSail/bnck';
export const RELEASES_URL = 'https://github.com/NightSail/bnck/releases';
export const LICENSE_URL = 'https://opensource.org/license/isc';

export const HTML_CSP = "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; " +
	"form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

export function channelLabel(channel: string): string {
	switch (channel) {
		case 'arch':
			return 'Arch Linux';
		case 'debian':
			return 'Debian';
		case 'fedora':
			return 'Fedora';
		case 'homebrew':
			return 'Homebrew';
		case 'pypi':
			return 'PyPI';
		default:
			return channel;
	}
}
