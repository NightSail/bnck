import type { ProviderExport } from './types';

export function formatJsonExport(payload: ProviderExport): string {
	return JSON.stringify(payload, null, '\t') + '\n';
}

export function mountJsonExport(root: ParentNode, payload: ProviderExport): void {
	const actions = root.querySelector<HTMLElement>('[data-json-export]');
	const button = actions?.querySelector<HTMLButtonElement>('[data-copy-json]');
	const view = actions?.querySelector<HTMLButtonElement>('[data-view-json]');
	const status = actions?.querySelector<HTMLElement>('[role="status"]');
	if (!actions || !button || !view || !status) return;

	const text = formatJsonExport(payload);
	let details: HTMLDetailsElement | undefined;
	view.addEventListener('click', () => {
		if (!details) {
			details = document.createElement('details');
			details.className = 'json-export-content';
			const summary = document.createElement('summary');
			summary.textContent = 'JSON export';
			const pre = document.createElement('pre');
			pre.textContent = text;
			details.append(summary, pre);
			actions.after(details);
		}
		details.open = !details.open;
	});

	const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard;
	const supportsItems = typeof ClipboardItem === 'function' && typeof clipboard?.write === 'function';
	const supportsText = typeof clipboard?.writeText === 'function';
	if (typeof window === 'undefined' || !window.isSecureContext || (!supportsItems && !supportsText)) return;

	button.hidden = false;
	button.addEventListener('click', async () => {
		if (button.disabled) return;
		button.disabled = true;
		button.textContent = 'copying…';
		status.classList.add('visually-hidden');
		status.textContent = 'copying JSON…';
		try {
			if (supportsItems) {
				const content = new Blob([text], { type: 'text/plain' });
				await clipboard!.write([new ClipboardItem({ 'text/plain': content })]);
			} else {
				await clipboard!.writeText(text);
			}
			button.textContent = 'JSON copied';
			status.textContent = 'JSON copied.';
			setTimeout(() => {
				button.textContent = 'copy JSON';
				status.textContent = '';
			}, 2000);
		} catch {
			button.textContent = 'copy JSON';
			status.classList.remove('visually-hidden');
			status.textContent = 'Copy failed. Use view JSON to copy manually.';
		} finally {
			button.disabled = false;
		}
	});
}
