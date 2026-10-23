import { realpath, stat } from 'node:fs/promises';
import { resolve, sep } from 'node:path';

import { contentTypeForPath, staticResponse } from './http';

export interface StaticFile {
	file: ReturnType<typeof Bun.file>;
	path: string;
}

export async function readStaticFile(root: string, pathname: string): Promise<StaticFile | null> {
	try {
		let resolved = await realpath(resolve(root, `.${pathname}`));
		if (!resolved.startsWith(`${root}${sep}`) && resolved !== root) return null;
		if ((await stat(resolved)).isDirectory()) resolved = await realpath(resolve(resolved, 'index.html'));
		if (!resolved.startsWith(`${root}${sep}`)) return null;
		return { file: Bun.file(resolved), path: resolved };
	} catch (error) {
		if (['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '')) return null;
		throw error;
	}
}

export function staticFileResponse(file: StaticFile, request: Request, options: {
	status?: number;
	immutable?: boolean;
} = {}): Response {
	const contentLength = file.file.size;
	return staticResponse(request.method === 'HEAD' ? null : file.file, {
		status: options.status ?? 200,
		contentType: contentTypeForPath(file.path),
		contentLength,
		cacheControl: options.immutable ? 'public, max-age=31536000, immutable' : 'no-cache',
	});
}

export async function notFoundResponse(root: string, request: Request): Promise<Response> {
	return fileOrDefault(await readStaticFile(root, '/404.html'), 'Not found', request, 404);
}

function fileOrDefault(file: StaticFile | null, fallback: string, request: Request,
	status: number): Response {
	if (file) return staticFileResponse(file, request, { status });
	const body = new TextEncoder().encode(fallback);
	return staticResponse(request.method === 'HEAD' ? null : body, {
		status,
		contentType: 'text/plain; charset=utf-8',
		contentLength: body.length,
	});
}
