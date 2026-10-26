use zed_extension_api::{self as zed, Result, Worktree, serde_json::Value};

use crate::platform::Platform;

const HOISTED_LOCKFILES: [&str; 3] = ["package-lock.json", "yarn.lock", "bun.lock"];
const EFFECT_TSGO: &str = "@effect/tsgo";

/// Prints the bundled tsrs path for `node -e <script> <root> <bundle>`, resolved from the
/// worktree root the way `effect-tsgo tsrs` does.
const RESOLVE_SCRIPT: &str = r#"
const { createRequire } = require("node:module");
const fs = require("node:fs");
const path = require("node:path");
const [root, bundle] = process.argv.slice(1);
const fromRoot = createRequire(path.join(root, "noop.js"));
const tsgo = fromRoot.resolve("@effect/tsgo/package.json");
const dir = path.dirname(createRequire(tsgo).resolve(`${bundle}/package.json`));
const upstream = JSON.parse(fs.readFileSync(path.join(dir, "lib", "upstream.json"), "utf8"));
let version;
try { version = fromRoot("tsrs/package.json").version; } catch {}
if (!upstream.components.tsrs[version]) version = upstream.tags.tsrs.latest;
process.stdout.write(path.join(dir, "artifacts", "tsrs", version, "tsrs"));
"#;

/// Returns the absolute path of the worktree's Effect-patched tsrs, preferring, in order:
///
/// 1. the build bundled in `@effect/tsgo-<platform>`, which needs no patching;
/// 2. a `@tsrs/<platform>` binary patched by `effect-tsgo patch --tsrs`;
/// 3. the bundled build as resolved by Zed's node, when `@effect/tsgo` is a dependency;
/// 4. the package manager's conventional `@tsrs/<platform>` path.
///
/// Package manifests are read first, so npm, yarn, bun and pnpm layouts are found without
/// running node. Zed can only read indexed files, which usually excludes gitignored and
/// symlinked `node_modules`; that is what the node lookup and the unchecked conventional
/// path cover, and Zed reports the spawn error if the binary is missing.
pub fn find(worktree: &Worktree) -> Result<String> {
    let platform = Platform::current()?;
    let root = worktree.root_path();
    let root = root.trim_end_matches(['/', '\\']);

    if let Some(bundle) = &platform.bundle
        && let Some(path) = find_bundled(worktree, bundle)
    {
        return Ok(format!("{root}/{path}"));
    }
    if let Some(dir) = find_patched(worktree, &platform) {
        return Ok(format!("{root}/{dir}/{}", platform.exe_name));
    }
    if let Some(bundle) = &platform.bundle
        && declares(worktree, EFFECT_TSGO)
        && let Some(path) = resolve_bundled(root, bundle)
    {
        return Ok(path);
    }
    let dir = conventional_dir(worktree, &platform.packages[0]);
    Ok(format!("{root}/{dir}/{}", platform.exe_name))
}

/// Runs node once to resolve the bundled tsrs; the server itself is started without it.
fn resolve_bundled(root: &str, bundle: &str) -> Option<String> {
    let output = zed::process::Command::new(zed::node_binary_path().ok()?)
        .args(["-e", RESOLVE_SCRIPT, root, bundle])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?;
    (output.status == Some(0) && !path.is_empty()).then_some(path)
}

/// The bundled tsrs, chosen as `effect-tsgo tsrs` does: the build matching the installed
/// `tsrs` package if one is bundled, else the latest.
fn find_bundled(worktree: &Worktree, bundle: &str) -> Option<String> {
    let version = declared_versions(
        worktree,
        &format!("node_modules/{EFFECT_TSGO}/package.json"),
    )
    .into_iter()
    .find(|(name, _)| name == bundle)
    .map(|(_, version)| version);
    let dir = find_package(
        worktree,
        bundle,
        &[format!("node_modules/{EFFECT_TSGO}/node_modules/{bundle}")],
        version.as_deref(),
    )?;
    let upstream = read_json(worktree, &format!("{dir}/lib/upstream.json"))?;
    let installed = read_json(worktree, "node_modules/tsrs/package.json")
        .and_then(|manifest| manifest.get("version")?.as_str().map(str::to_owned));
    let version = installed
        .filter(|v| upstream.pointer(&format!("/components/tsrs/{v}")).is_some())
        .or_else(|| Some(upstream.pointer("/tags/tsrs/latest")?.as_str()?.to_owned()))?;
    Some(format!("{dir}/artifacts/tsrs/{version}/tsrs"))
}

fn find_patched(worktree: &Worktree, platform: &Platform) -> Option<String> {
    let declared = declared_versions(worktree, "node_modules/tsrs/package.json");
    platform.packages.iter().find_map(|package| {
        let version = declared
            .iter()
            .find(|(name, _)| name == package)
            .map(|(_, version)| version.as_str());
        find_package(
            worktree,
            package,
            &[format!("node_modules/tsrs/node_modules/{package}")],
            version,
        )
    })
}

/// Finds an installed package's directory by its manifest. `nested` lists the npm
/// locations under its parent package; `store_version` names pnpm's versioned store
/// directory.
fn find_package(
    worktree: &Worktree,
    package: &str,
    nested: &[String],
    store_version: Option<&str>,
) -> Option<String> {
    let mut dirs = vec![
        format!("node_modules/{package}"),
        format!("node_modules/.pnpm/node_modules/{package}"),
    ];
    dirs.extend_from_slice(nested);
    if let Some(version) = store_version {
        let store_name = package.replace('/', "+");
        dirs.push(format!(
            "node_modules/.pnpm/{store_name}@{version}/node_modules/{package}"
        ));
    }
    dirs.into_iter().find(|dir| {
        read_json(worktree, &format!("{dir}/package.json"))
            .and_then(|manifest| manifest.get("name")?.as_str().map(str::to_owned))
            .is_some_and(|name| name == package)
    })
}

/// Platform package versions pinned in a package's `optionalDependencies`, which name the
/// versioned directory in pnpm's store.
fn declared_versions(worktree: &Worktree, manifest_path: &str) -> Vec<(String, String)> {
    read_json(worktree, manifest_path)
        .and_then(|manifest| {
            let deps = manifest.get("optionalDependencies")?.as_object()?;
            Some(
                deps.iter()
                    .filter_map(|(name, version)| {
                        Some((name.clone(), version.as_str()?.to_owned()))
                    })
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// Whether the worktree's own `package.json`, which is tracked and so readable, depends on
/// `package`.
fn declares(worktree: &Worktree, package: &str) -> bool {
    read_json(worktree, "package.json").is_some_and(|manifest| {
        ["dependencies", "devDependencies"].iter().any(|field| {
            manifest
                .get(field)
                .and_then(|deps| deps.get(package))
                .is_some()
        })
    })
}

/// Lockfiles are tracked, so Zed can read them even when `node_modules` is hidden.
fn conventional_dir(worktree: &Worktree, package: &str) -> String {
    let readable = |path: &str| worktree.read_text_file(path).is_ok();
    let hoisted = !readable("pnpm-lock.yaml") && HOISTED_LOCKFILES.iter().any(|f| readable(f));
    if hoisted {
        format!("node_modules/{package}")
    } else {
        format!("node_modules/.pnpm/node_modules/{package}")
    }
}

fn read_json(worktree: &Worktree, path: &str) -> Option<Value> {
    let text = worktree.read_text_file(path).ok()?;
    zed_extension_api::serde_json::from_str(&text).ok()
}
