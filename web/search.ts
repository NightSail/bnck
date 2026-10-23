const search = document.querySelector<HTMLInputElement>('#q');

if (search) {
	search.setAttribute('aria-keyshortcuts', '/');
	search.title = 'press / to focus search';
	document.addEventListener('keydown', (event) => {
		const current = document.querySelector<HTMLInputElement>('#q');
		if (!current || event.key !== '/' || event.ctrlKey || event.altKey || event.metaKey || event.isComposing ||
			event.defaultPrevented || current.disabled || current.hidden) {
			return;
		}
		const isEditing = event.composedPath().some((target) => target instanceof HTMLElement &&
			(target.isContentEditable || target.matches('input, textarea, select')));
		if (isEditing) {
			return;
		}
		event.preventDefault();
		current.focus();
		current.select();
	});
}
