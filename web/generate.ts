import { parseArgs } from 'node:util';

import { generateSite } from './site';

const { values } = parseArgs({
	options: {
		database: { type: 'string', default: 'var/bnck.sqlite.zst' },
		output: { type: 'string', default: 'var/site' },
		'live-reload': { type: 'boolean', default: false },
	},
});

await generateSite(values.database, values.output, { liveReload: values['live-reload'] === true });
console.error(`wrote static website to ${values.output}`);
