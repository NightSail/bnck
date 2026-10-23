export function foldAscii(value: string): string {
	return value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

export function compareNames(left: string, right: string): number {
	/* SQLite BINARY order follows Unicode scalar values, not JavaScript's UTF-16 code units or locale. */
	const a = [...left];
	const b = [...right];
	for (let index = 0; index < Math.min(a.length, b.length); index++) {
		const difference = a[index].codePointAt(0)! - b[index].codePointAt(0)!;
		if (difference) return difference;
	}
	return a.length - b.length;
}

export function encodeSegment(value: string): string {
	return encodeURIComponent(value).replace(/[!'()*]/g, (character) =>
		`%${character.charCodeAt(0).toString(16).toUpperCase()}`);
}

export function encodePackagePath(packageName: string): string {
	return packageName.split('/').map(encodeSegment).join('/');
}

export function formatNumber(value: number): string {
	return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

export function canonicalPackageUrl(channel: string, packageName: string): string | null {
	const packagePath = encodePackagePath(packageName);
	switch (channel) {
		case 'arch':
			return `https://archlinux.org/packages/?q=${encodeSegment(packageName)}`;
		case 'crates.io':
			return `https://crates.io/crates/${packagePath}`;
		case 'debian':
			return `https://packages.debian.org/${packagePath}`;
		case 'fedora':
			return `https://packages.fedoraproject.org/pkgs/${packagePath}/${packagePath}/`;
		case 'homebrew':
			return homebrewPackageUrl(packageName);
		case 'npm':
			return `https://www.npmjs.com/package/${packagePath}`;
		case 'pypi':
			return `https://pypi.org/project/${packagePath}/`;
		default:
			return null;
	}
}

function homebrewPackageUrl(packageName: string): string | null {
	const formula = packageName.startsWith('homebrew/core/') ? packageName.slice('homebrew/core/'.length) : null;
	if (formula !== null) return `https://formulae.brew.sh/formula/${encodeSegment(formula)}.html`;
	const cask = packageName.startsWith('homebrew/cask/') ? packageName.slice('homebrew/cask/'.length) : null;
	return cask === null ? null : `https://formulae.brew.sh/cask/${encodeSegment(cask)}.html`;
}
