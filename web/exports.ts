const supportsClipboardItems = typeof ClipboardItem === 'function' && typeof navigator.clipboard?.write === 'function';

if (window.isSecureContext && (supportsClipboardItems || typeof navigator.clipboard?.writeText === 'function')) {
	for (const actions of document.querySelectorAll<HTMLElement>('[data-json-export]')) {
		const button = actions.querySelector<HTMLButtonElement>('[data-copy-json]');
		const view = actions.querySelector<HTMLAnchorElement>('[data-view-json]');
		const status = actions.querySelector<HTMLElement>('[role="status"]');
		if (!button || !view || !status) continue;
		let restoreLabelTimeout: ReturnType<typeof setTimeout> | undefined;
		button.addEventListener('click', async () => {
			if (button.disabled) {
				return;
			}
			clearTimeout(restoreLabelTimeout);
			const isCopied = await copyJson(view.href, button, status);
			if (isCopied) {
				restoreLabelTimeout = setTimeout(() => {
					button.textContent = 'copy JSON';
					status.textContent = '';
				}, 2000);
			}
		});
		button.hidden = false;
	}
}

async function copyJson(url: string, button: HTMLButtonElement, status: HTMLElement): Promise<boolean> {
	button.disabled = true;
	button.textContent = 'copying…';
	status.classList.add('visually-hidden');
	status.textContent = 'copying JSON…';
	const controller = new AbortController();
	const timeout = setTimeout(() => controller.abort(), 10000);
	try {
		const content = loadJson(url, controller.signal);
		await Promise.all([content, writeClipboard(content)]);
		button.textContent = 'JSON copied';
		status.textContent = 'JSON copied.';
		return true;
	} catch {
		controller.abort();
		button.textContent = 'copy JSON';
		status.classList.remove('visually-hidden');
		status.textContent = 'Copy failed. Use view JSON to copy manually.';
		return false;
	} finally {
		clearTimeout(timeout);
		button.disabled = false;
	}
}

async function loadJson(url: string, signal: AbortSignal): Promise<Blob> {
	const response = await fetch(url, {
		headers: { Accept: 'application/json' },
		credentials: 'same-origin',
		redirect: 'error',
		signal,
	});
	if (!response.ok) {
		throw new Error(`JSON export returned HTTP ${response.status}`);
	}
	const data = await response.json();
	return new Blob([JSON.stringify(data, null, '\t') + '\n'], { type: 'text/plain' });
}

async function writeClipboard(content: Promise<Blob>): Promise<void> {
	if (supportsClipboardItems) {
		/* Start the write during the click gesture; the item waits for the fetch on browsers such as Safari. */
		await navigator.clipboard.write([new ClipboardItem({ 'text/plain': content })]);
	} else {
		const blob = await content;
		await navigator.clipboard.writeText(await blob.text());
	}
}
