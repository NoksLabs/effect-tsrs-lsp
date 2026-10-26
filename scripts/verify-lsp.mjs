#!/usr/bin/env node
// Drives the worktree's tsrs over LSP exactly as the Zed extension launches it:
// same binary resolution, args ["--lsp", "--stdio"], cwd = worktree root, and an
// environment without node on PATH. The probe file is sent via didOpen only and is
// never written to disk.
//
// Usage: node scripts/verify-lsp.mjs <worktree-root> [relative/probe/path.ts]

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = path.resolve(process.argv[2] ?? ".");
const probeRel = process.argv[3] ?? "src/__effect_tsrs_probe__.ts";

// Mirrors find_worktree_binary in src/tsrs.rs.
function resolveBinary() {
	const readJson = (rel) => {
		try {
			return JSON.parse(fs.readFileSync(path.join(root, rel), "utf8"));
		} catch {
			return undefined;
		}
	};
	const arch = { arm64: "arm64", x64: "x64" }[process.arch];
	const prefixes = {
		darwin: [`darwin-${arch}`],
		linux: [`linux-${arch}-gnu`, `linux-${arch}-musl`],
		win32: [`win32-${arch}`],
	}[process.platform];
	const exe = process.platform === "win32" ? "tsrs.exe" : "tsrs";
	const declared = Object.entries(readJson("node_modules/tsrs/package.json")?.optionalDependencies ?? {});
	const packages = prefixes.flatMap((prefix) => {
		const full = `@tsrs/${prefix}`;
		const matches = declared.filter(([name]) => name === full || name.startsWith(`${full}-`));
		return matches.length ? matches : [[full, undefined]];
	});
	for (const [pkg, version] of packages) {
		const dirs = [
			`node_modules/${pkg}`,
			`node_modules/.pnpm/node_modules/${pkg}`,
			`node_modules/tsrs/node_modules/${pkg}`,
		];
		if (version) dirs.push(`node_modules/.pnpm/${pkg.replace("/", "+")}@${version}/node_modules/${pkg}`);
		for (const dir of dirs) {
			if (readJson(`${dir}/package.json`)?.name === pkg) return `${root}/${dir}/${exe}`;
		}
	}
	throw new Error("no tsrs platform package found");
}

const binary = resolveBinary();
console.log(`binary: ${binary}`);

const child = spawn(binary, ["--lsp", "--stdio"], {
	cwd: root,
	env: { PATH: "/usr/bin:/bin", HOME: process.env.HOME },
	stdio: ["pipe", "pipe", "pipe"],
});
child.stderr.on("data", (d) => process.stderr.write(`[stderr] ${d}`));

let buffer = Buffer.alloc(0);
let nextId = 1;
const pending = new Map();
const notifications = [];
const waiters = [];

child.stdout.on("data", (chunk) => {
	buffer = Buffer.concat([buffer, chunk]);
	for (;;) {
		const headerEnd = buffer.indexOf("\r\n\r\n");
		if (headerEnd < 0) return;
		const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, headerEnd).toString())[1]);
		if (buffer.length < headerEnd + 4 + length) return;
		const msg = JSON.parse(buffer.subarray(headerEnd + 4, headerEnd + 4 + length).toString());
		buffer = buffer.subarray(headerEnd + 4 + length);
		if (msg.id !== undefined && msg.method) {
			// Server-to-client request: answer workspace/configuration with empty settings,
			// like the extension does when no lsp.effect-tsrs.settings are configured.
			const result = msg.method === "workspace/configuration" ? msg.params.items.map(() => ({})) : null;
			send({ jsonrpc: "2.0", id: msg.id, result });
		} else if (msg.id !== undefined) {
			pending.get(msg.id)?.(msg);
			pending.delete(msg.id);
		} else {
			notifications.push(msg);
			for (const w of [...waiters]) w();
		}
	}
});

function send(msg) {
	const body = Buffer.from(JSON.stringify(msg));
	child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
	child.stdin.write(body);
}
function request(method, params, timeoutMs = 60000) {
	const id = nextId++;
	return new Promise((resolve, reject) => {
		const t = setTimeout(() => reject(new Error(`${method} timed out`)), timeoutMs);
		pending.set(id, (m) => {
			clearTimeout(t);
			m.error ? reject(new Error(`${method}: ${JSON.stringify(m.error)}`)) : resolve(m.result);
		});
		send(params === undefined ? { jsonrpc: "2.0", id, method } : { jsonrpc: "2.0", id, method, params });
	});
}
const notify = (method, params) => send({ jsonrpc: "2.0", method, params });
function waitFor(pred, timeoutMs) {
	return new Promise((resolve) => {
		const check = () => {
			const hit = notifications.find(pred);
			if (hit) {
				waiters.splice(waiters.indexOf(check), 1);
				resolve(hit);
			}
		};
		waiters.push(check);
		setTimeout(() => resolve(undefined), timeoutMs);
		check();
	});
}

const failures = [];
const check = (ok, label) => {
	console.log(`${ok ? "PASS" : "FAIL"} ${label}`);
	if (!ok) failures.push(label);
};

const probeText = [
	`import { Effect } from "effect"`,
	``,
	`const program = Effect.succeed(42)`,
	``,
	`export function run() {`,
	`  Effect.log("floating")`,
	`  return program`,
	`}`,
	``,
].join("\n");
const probeUri = pathToFileURL(path.join(root, probeRel)).href;

try {
	const init = await request("initialize", {
		processId: process.pid,
		rootUri: pathToFileURL(root).href,
		workspaceFolders: [{ uri: pathToFileURL(root).href, name: path.basename(root) }],
		initializationOptions: {},
		capabilities: {
			textDocument: {
				hover: { contentFormat: ["markdown", "plaintext"] },
				publishDiagnostics: { relatedInformation: true },
				diagnostic: { dynamicRegistration: false },
				synchronization: { didSave: true },
			},
			workspace: { configuration: true, workspaceFolders: true },
		},
	});
	console.log(`serverInfo: ${JSON.stringify(init.serverInfo)}`);
	check(/effect-tsgo/.test(init.serverInfo?.version ?? "") || /effect-tsgo/.test(init.serverInfo?.name ?? ""), "server is the Effect-patched build");
	const caps = init.capabilities;
	for (const cap of ["hoverProvider", "completionProvider", "definitionProvider", "signatureHelpProvider", "inlayHintProvider", "codeActionProvider"]) {
		check(Boolean(caps[cap]), `advertises ${cap}`);
	}
	notify("initialized", {});
	notify("workspace/didChangeConfiguration", { settings: {} });
	notify("textDocument/didOpen", {
		textDocument: { uri: probeUri, languageId: "typescript", version: 1, text: probeText },
	});

	console.log(`diagnosticProvider: ${JSON.stringify(caps.diagnosticProvider)}`);
	// Accept either model: pushed publishDiagnostics, or pulled textDocument/diagnostic.
	// The project loads asynchronously, so retry the pull until the Effect check shows up.
	const hasFloating = (ds) => ds.some((d) => String(d.code) === "377001");
	let diagnostics = [];
	const deadline = Date.now() + 120000;
	while (Date.now() < deadline && !hasFloating(diagnostics)) {
		const pushed = notifications.filter((n) => n.method === "textDocument/publishDiagnostics" && n.params.uri === probeUri);
		if (pushed.length) diagnostics = pushed.at(-1).params.diagnostics;
		if (!hasFloating(diagnostics) && caps.diagnosticProvider) {
			const report = await request("textDocument/diagnostic", { textDocument: { uri: probeUri } }, 120000);
			diagnostics = report.items ?? [];
		}
		if (!hasFloating(diagnostics)) await new Promise((r) => setTimeout(r, 1000));
	}
	for (const d of diagnostics) {
		console.log(`  diag ${d.code} L${d.range.start.line + 1}: ${String(d.message).split("\n")[0]}`);
	}
	check(hasFloating(diagnostics), "floating Effect reported as TS377001");

	const hover = await request("textDocument/hover", {
		textDocument: { uri: probeUri },
		position: { line: 2, character: 7 }, // `program`
	});
	const hoverText = JSON.stringify(hover?.contents ?? "");
	console.log(`hover: ${hoverText.slice(0, 300)}`);
	check(/Effect<number/.test(hoverText), "hover shows inferred type Effect<number, ...>");

	await request("shutdown");
	send({ jsonrpc: "2.0", method: "exit" });
} catch (err) {
	check(false, String(err));
	child.kill();
}

console.log(failures.length ? `\n${failures.length} check(s) failed` : "\nall checks passed");
process.exitCode = failures.length ? 1 : 0;
setTimeout(() => process.exit(), 500);
