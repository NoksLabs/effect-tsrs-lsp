# effect-tsrs-zed

Zed extension that runs the Effect-patched [tsrs](https://github.com/oxc-project/tsrs)
(the Rust port of TypeScript 7) as the language server for TypeScript, TSX, JavaScript
and JSX. You get full type inference (hover types, completions, go-to-definition,
signature help, inlay hints, code actions) together with the Effect language service
diagnostics and quick info.

The extension never downloads anything. It runs the tsrs binary that your project
already has, after `effect-tsgo patch --tsrs` has replaced it with the Effect build.

## Binary resolution

1. `lsp.effect-tsrs.binary.path` from Zed settings, with `binary.arguments` (default
   `["--lsp", "--stdio"]`) and `binary.env`.
2. The worktree's native platform package `@tsrs/<platform>` (for example
   `@tsrs/darwin-arm64`). It is found by reading package manifests, so node is not
   needed. Layouts checked, in order:
   - `node_modules/@tsrs/<platform>` (npm, yarn, bun, pnpm `node-linker=hoisted`)
   - `node_modules/.pnpm/node_modules/@tsrs/<platform>` (pnpm virtual store)
   - `node_modules/tsrs/node_modules/@tsrs/<platform>` (npm nested)
   - `node_modules/.pnpm/@tsrs+<platform>@<version>/node_modules/@tsrs/<platform>`, where
     `<version>` comes from `node_modules/tsrs/package.json`
3. If none of those can be read, the extension uses the conventional path without
   checking it. Zed can only read files in its worktree index, which usually leaves out
   gitignored and symlinked `node_modules`, so in Zed this fallback is the usual case.
   The path is `<worktree>/node_modules/.pnpm/node_modules/@tsrs/<platform>/tsrs` (pnpm's
   version-free link), or `<worktree>/node_modules/@tsrs/<platform>/tsrs` when the
   worktree has a `package-lock.json`, `yarn.lock` or `bun.lock` and no `pnpm-lock.yaml`.
   If the binary is missing, Zed shows the spawn error. In that case run `pnpm install`
   and `effect-tsgo patch --tsrs`, or set `binary.path`.

> **Note:** when `binary.path` is set, Zed launches that binary itself with
> `binary.arguments`, and it may do so without asking the extension. Always set
> `"arguments": ["--lsp", "--stdio"]` together with `path`. Without them, tsrs starts
> as a plain compiler and Zed reports `Server reset the connection`.

`initialization_options` and `settings` under `lsp.effect-tsrs` are passed through to
the server unchanged.

## Requirements

- Rust installed with rustup, plus the `wasm32-wasip2` target:
  `rustup target add wasm32-wasip2`. Zed must find the rustup `cargo` before any other
  Rust install on `PATH`.
- The project has `tsrs` installed and patched (`effect-tsgo patch --tsrs`).
- The `@effect/language-service` plugin is listed in `tsconfig.json`. Effect
  diagnostics only appear when the plugin option `"diagnostics"` is not `false`.

## Install in Zed

1. Open the Extensions page (`zed: extensions`).
2. Click **Install Dev Extension** (or run `zed: install dev extension`).
3. Select this repository's directory. Zed builds it for `wasm32-wasip2` and loads it.
4. Enable it per language in `.zed/settings.json`:

```jsonc
{
  "languages": {
    "TypeScript": { "language_servers": ["effect-tsrs", "!vtsls", "!typescript-language-server", "!effect-tsgo"] },
    "TSX": { "language_servers": ["effect-tsrs", "!vtsls", "!typescript-language-server", "!effect-tsgo"] }
  }
}
```

To check that it started, run `dev: open language server logs` and pick `effect-tsrs`.

## Verifying outside Zed

`scripts/verify-lsp.mjs` resolves the binary with the same rules as the extension and
launches it the way Zed does: args `--lsp --stdio`, cwd set to the worktree root, and
no node on `PATH`. It then checks `initialize`, the advertised capabilities, the
diagnostics for a probe file containing a floating Effect (TS377001), and a hover. The
probe file is sent with `didOpen` only and is never written to disk.

```sh
node scripts/verify-lsp.mjs /path/to/worktree packages/foo/src/probe.ts
```

## License

Apache-2.0. Parts are adapted from
[zed-effect-tsgo](https://github.com/RATIU5/zed-effect-tsgo) and the Zed tsgo extension;
see `LICENSE` and `NOTICE`.
