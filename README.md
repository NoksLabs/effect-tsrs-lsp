# Effect tsrs for Zed

A [Zed](https://zed.dev) extension that uses the Effect-patched
[tsrs](https://github.com/oxc-project/tsrs) (a Rust port of the TypeScript 7 compiler)
as the language server for TypeScript, TSX, JavaScript and JSX.

One server provides full type information (hover, completions, go to definition,
signature help, inlay hints, code actions) and the
[Effect language service](https://github.com/Effect-TS/language-service): Effect
diagnostics, quick info and refactors.

The extension does not download anything. It runs the Effect-patched tsrs that
`@effect/tsgo` installs in your project.

## Project setup

1. Install `@effect/tsgo` 0.59 or later:

   ```sh
   pnpm add -D @effect/tsgo
   ```

   It ships a ready-built, Effect-patched tsrs for macOS arm64 and Linux x64/arm64
   (glibc). No `tsrs` package and no patch step are needed.

   On other platforms, install `tsrs` and patch it instead:

   ```sh
   pnpm add -D tsrs @effect/tsgo
   pnpm exec effect-tsgo patch --tsrs
   ```

2. Enable the plugin in `tsconfig.json`:

   ```jsonc
   {
     "compilerOptions": {
       "plugins": [{ "name": "@effect/language-service" }]
     }
   }
   ```

   Effect diagnostics are hidden in the editor while the plugin option `"diagnostics"`
   is `false`.

## Installation

Open the Extensions page in Zed (`zed: extensions`), search for **Effect tsrs**, and
click Install.

### Development build

1. Install Rust with [rustup](https://rustup.rs) and add the WebAssembly target:

   ```sh
   rustup target add wasm32-wasip2
   ```

   Zed must find rustup's `cargo` before any other Rust installation on `PATH`.

2. Clone this repository.
3. In Zed, open the Extensions page and click **Install Dev Extension** (or run
   `zed: install dev extension`), then select the cloned directory.

To update later, pull the repository and click **Rebuild** on the extension.

## Configuration

Use `effect-tsrs` for your languages and turn off the other TypeScript servers so
diagnostics are not duplicated:

```jsonc
// .zed/settings.json
{
  "languages": {
    "TypeScript": {
      "language_servers": ["effect-tsrs", "!vtsls", "!typescript-language-server", "..."]
    },
    "TSX": {
      "language_servers": ["effect-tsrs", "!vtsls", "!typescript-language-server", "..."]
    }
  }
}
```

`lsp.effect-tsrs.initialization_options` and `lsp.effect-tsrs.settings` are passed to the
server unchanged.

### Custom binary

This is optional; the extension finds the binary on its own. To run a different binary,
set its path. When you do, Zed starts the binary directly
with `arguments`, so the arguments must be set too:

```jsonc
{
  "lsp": {
    "effect-tsrs": {
      "binary": {
        "path": "/absolute/path/to/tsrs",
        "arguments": ["--lsp", "--stdio"]
      }
    }
  }
}
```

If the arguments are missing, tsrs runs as a plain compiler and exits, and Zed reports
`Server reset the connection`.

## How the binary is found

Without `binary.path`, the extension tries, in order:

1. The tsrs bundled in `@effect/tsgo-<platform>`, at
   `artifacts/tsrs/<version>/tsrs`. Like `effect-tsgo tsrs`, it picks the build matching
   the installed `tsrs` package if one is bundled, and the latest otherwise.
2. A patched `@tsrs/<platform>` package (for example `@tsrs/darwin-arm64`).
3. The bundled tsrs again, located by running Zed's node once, if the worktree's
   `package.json` depends on `@effect/tsgo`.
4. The standard `@tsrs/<platform>` location, unchecked.

Steps 1 and 2 read package manifests, so node is not needed. They look in:

- `node_modules/<package>` (npm, yarn, bun, pnpm with `node-linker=hoisted`)
- `node_modules/.pnpm/node_modules/<package>` (pnpm)
- `node_modules/@effect/tsgo/node_modules/<package>` or
  `node_modules/tsrs/node_modules/<package>` (nested npm install)
- `node_modules/.pnpm/<scope>+<name>@<version>/node_modules/<package>` (pnpm store, with
  the version taken from the parent package's `optionalDependencies`)

Zed usually can't read gitignored or symlinked `node_modules`, which is what step 3
covers. Node only runs for the lookup, which takes a few tens of milliseconds; the server
runs without it. Step 4 uses `node_modules/@tsrs/<platform>` if the worktree has a
`package-lock.json`, `yarn.lock` or `bun.lock` and no `pnpm-lock.yaml`, and
`node_modules/.pnpm/node_modules/@tsrs/<platform>` otherwise.

## Troubleshooting

- Run `dev: open language server logs` and pick `effect-tsrs` to see what the server
  received.
- `zed: open log` shows the command Zed tried to start and any spawn error. If the binary
  is missing, run your package manager's install (and `effect-tsgo patch --tsrs` again if
  you use a patched `tsrs`).
- If hover works but no Effect diagnostics show up, check that the plugin is configured
  and that `"diagnostics"` is not `false`.

## Development

`scripts/verify-lsp.mjs` checks a project without Zed. It finds the binary the same way
the extension does, starts it with `--lsp --stdio`, and checks the server's
capabilities, a floating-Effect diagnostic (TS377001) and a hover:

```sh
node scripts/verify-lsp.mjs /path/to/project src/probe.ts
```

Add `--node` to skip the manifest reads and test the node lookup, as when Zed hasn't
indexed `node_modules`.

The probe file is sent to the server in memory and never written to disk.

## License

[Apache-2.0](LICENSE). Adapted from
[zed-effect-tsgo](https://github.com/RATIU5/zed-effect-tsgo), which is based on the Zed
tsgo extension; see [NOTICE](NOTICE).
