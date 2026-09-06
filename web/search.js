const search = document.querySelector('#q');

if (search) {
	search.setAttribute('aria-keyshortcuts', '/');
	search.title = 'Press / to focus search';
	document.addEventListener('keydown', (event) => {
		if (event.key !== '/' || event.ctrlKey || event.altKey || event.metaKey || event.isComposing ||
			event.defaultPrevented || search.disabled || search.hidden) {
			return;
		}
		const isEditing = event.composedPath().some((target) => target instanceof HTMLElement &&
			(target.isContentEditable || target.matches('input, textarea, select')));
		if (isEditing) {
			return;
		}
		event.preventDefault();
		search.focus();
		search.select();
	});
}
