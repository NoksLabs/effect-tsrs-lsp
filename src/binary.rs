use zed_extension_api::{Result, Worktree, serde_json::Value};

use crate::platform::Platform;

const HOISTED_LOCKFILES: [&str; 3] = ["package-lock.json", "yarn.lock", "bun.lock"];

/// Returns the absolute path of the worktree's native tsrs binary.
///
/// Reads package manifests first, so npm, yarn, bun and pnpm layouts are found without
/// running node. Zed can only read indexed files, which usually excludes gitignored and
/// symlinked `node_modules`; in that case the package manager's conventional path is
/// returned unchecked, and Zed reports the spawn error if the binary is missing.
pub fn find(worktree: &Worktree) -> Result<String> {
    let platform = Platform::current()?;
    let package_dir = find_installed(worktree, &platform)
        .unwrap_or_else(|| conventional_dir(worktree, &platform.packages[0]));
    let root = worktree.root_path();
    let root = root.trim_end_matches(['/', '\\']);
    Ok(format!("{root}/{package_dir}/{}", platform.exe_name))
}

fn find_installed(worktree: &Worktree, platform: &Platform) -> Option<String> {
    let declared = declared_versions(worktree);
    for package in &platform.packages {
        let version = declared
            .iter()
            .find(|(name, _)| name == package)
            .map(|(_, version)| version);
        let mut dirs = vec![
            format!("node_modules/{package}"),
            format!("node_modules/.pnpm/node_modules/{package}"),
            format!("node_modules/tsrs/node_modules/{package}"),
        ];
        if let Some(version) = version {
            let store_name = package.replace('/', "+");
            dirs.push(format!(
                "node_modules/.pnpm/{store_name}@{version}/node_modules/{package}"
            ));
        }
        let found = dirs.into_iter().find(|dir| {
            read_json(worktree, &format!("{dir}/package.json"))
                .and_then(|manifest| manifest.get("name")?.as_str().map(str::to_owned))
                .is_some_and(|name| &name == package)
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Platform package versions pinned in `tsrs`'s `optionalDependencies`, which name the
/// versioned directory in pnpm's store.
fn declared_versions(worktree: &Worktree) -> Vec<(String, String)> {
    read_json(worktree, "node_modules/tsrs/package.json")
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
