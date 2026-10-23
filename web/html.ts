export class Html {
	constructor(readonly value: string) {}
}

export function raw(value: string): Html {
	return new Html(value);
}

export function escapeHtml(value: unknown): string {
	return String(value).replace(/[&<>"']/g, (character) => ({
		'&': '&amp;',
		'<': '&lt;',
		'>': '&gt;',
		'"': '&quot;',
		"'": '&#39;',
	})[character]!);
}

function renderValue(value: unknown): string {
	if (value instanceof Html) return value.value;
	if (Array.isArray(value)) return value.map(renderValue).join('');
	if (value === null || value === undefined || value === false) return '';
	return escapeHtml(value);
}

/* Escape every interpolation by default; wrap trusted fragments in Html to opt out. */
export function html(strings: TemplateStringsArray, ...values: unknown[]): Html {
	let output = strings[0];
	for (let index = 0; index < values.length; index++) {
		output += renderValue(values[index]);
		output += strings[index + 1];
	}
	return new Html(output);
}
