// Structure (settings passthrough, platform mapping, command shape) adapted from
// zed-effect-tsgo by RATIU5, itself based on the tsgo extension by Zed Industries, Inc.
// Both are licensed under the Apache License, Version 2.0; see LICENSE and NOTICE.

use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree, settings::LspSettings};

const TSRS_PACKAGE: &str = "tsrs";
const PLATFORM_SCOPE: &str = "@tsrs/";
const DEFAULT_ARGS: [&str; 2] = ["--lsp", "--stdio"];

struct EffectTsrsExtension;

/// Where the native binary was found, relative to the worktree root.
struct ResolvedBinary {
    package_dir: String,
    exe_name: &'static str,
}

impl EffectTsrsExtension {
    /// Prefixes of the `@tsrs/<platform>` package names that can run on this machine,
    /// in preference order. Linux has `-gnu`/`-musl` variants, which are told apart by
    /// whichever one the package manager actually installed.
    fn platform_prefixes() -> Result<(Vec<String>, &'static str)> {
        let (os, arch) = zed::current_platform();
        let arch = match arch {
            zed::Architecture::Aarch64 => "arm64",
            zed::Architecture::X8664 => "x64",
            zed::Architecture::X86 => {
                return Err("tsrs does not ship a 32-bit x86 binary.".to_string());
            }
        };
        Ok(match os {
            zed::Os::Mac => (vec![format!("darwin-{arch}")], "tsrs"),
            zed::Os::Linux => (
                vec![format!("linux-{arch}-gnu"), format!("linux-{arch}-musl")],
                "tsrs",
            ),
            zed::Os::Windows => (vec![format!("win32-{arch}")], "tsrs.exe"),
        })
    }

    fn read_json(worktree: &Worktree, path: &str) -> Option<zed::serde_json::Value> {
        let text = worktree.read_text_file(path).ok()?;
        zed::serde_json::from_str(&text).ok()
    }

    /// Finds the platform package directory of the worktree's tsrs install without
    /// running node. Checks the hoisted layout (npm, yarn, bun, pnpm with
    /// `node-linker=hoisted`), pnpm's virtual store, and npm's nested layout, then falls
    /// back to the package manager's conventional path without checking that it exists.
    fn find_worktree_binary(worktree: &Worktree) -> Result<ResolvedBinary> {
        let (prefixes, exe_name) = Self::platform_prefixes()?;
        let tsrs_manifest = Self::read_json(worktree, "node_modules/tsrs/package.json");

        // Exact platform package names and versions, as tsrs declares them.
        let declared: Vec<(String, Option<String>)> = tsrs_manifest
            .as_ref()
            .and_then(|m| m.get("optionalDependencies"))
            .and_then(|deps| deps.as_object())
            .map(|deps| {
                deps.iter()
                    .map(|(name, version)| (name.clone(), version.as_str().map(str::to_string)))
                    .collect()
            })
            .unwrap_or_default();

        let mut packages: Vec<(String, Option<String>)> = Vec::new();
        for prefix in &prefixes {
            let full = format!("{PLATFORM_SCOPE}{prefix}");
            let matches: Vec<_> = declared
                .iter()
                .filter(|(name, _)| name == &full || name.starts_with(&format!("{full}-")))
                .cloned()
                .collect();
            if matches.is_empty() {
                packages.push((full, None));
            } else {
                packages.extend(matches);
            }
        }

        for (package, version) in &packages {
            let mut dirs = vec![
                format!("node_modules/{package}"),
                format!("node_modules/.pnpm/node_modules/{package}"),
                format!("node_modules/{TSRS_PACKAGE}/node_modules/{package}"),
            ];
            if let Some(version) = version {
                let store_name = package.replace('/', "+");
                dirs.push(format!(
                    "node_modules/.pnpm/{store_name}@{version}/node_modules/{package}"
                ));
            }
            for dir in dirs {
                let manifest = Self::read_json(worktree, &format!("{dir}/package.json"));
                let name = manifest
                    .as_ref()
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str());
                if name == Some(package.as_str()) {
                    return Ok(ResolvedBinary {
                        package_dir: dir,
                        exe_name,
                    });
                }
            }
        }

        // Zed's read_text_file only sees files in the worktree index, which leaves out
        // gitignored and symlinked node_modules, so the probes above usually fail in the
        // editor. Fall back to the conventional location for the project's package manager,
        // found from its lockfile (lockfiles are tracked, so they can be read). If the binary
        // is not there, Zed reports the spawn error.
        let package = format!("{PLATFORM_SCOPE}{}", prefixes[0]);
        let hoisted_lockfiles = ["package-lock.json", "yarn.lock", "bun.lock"];
        let uses_hoisted_layout = worktree.read_text_file("pnpm-lock.yaml").is_err()
            && hoisted_lockfiles
                .iter()
                .any(|lockfile| worktree.read_text_file(lockfile).is_ok());
        let package_dir = if uses_hoisted_layout {
            format!("node_modules/{package}")
        } else {
            // pnpm's version-free link into the virtual store.
            format!("node_modules/.pnpm/node_modules/{package}")
        };
        Ok(ResolvedBinary {
            package_dir,
            exe_name,
        })
    }

    fn join(root: &str, relative: &str) -> String {
        let root = root.trim_end_matches(['/', '\\']);
        format!("{root}/{relative}")
    }
}

impl zed::Extension for EffectTsrsExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);

        let configured_path = binary
            .as_ref()
            .and_then(|b| b.path.as_deref())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        let configured_args = binary.as_ref().and_then(|b| b.arguments.clone());
        let env = binary
            .and_then(|b| b.env)
            .map(|env| env.into_iter().collect())
            .unwrap_or_default();

        let command = match configured_path {
            Some(path) => path,
            None => {
                let resolved = Self::find_worktree_binary(worktree)?;
                let relative = format!("{}/{}", resolved.package_dir, resolved.exe_name);
                Self::join(&worktree.root_path(), &relative)
            }
        };

        Ok(zed::Command {
            command,
            args: configured_args
                .unwrap_or_else(|| DEFAULT_ARGS.iter().map(|a| a.to_string()).collect()),
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let options = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options)
            .unwrap_or_default();
        Ok(Some(options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.settings)
            .unwrap_or_default();
        Ok(Some(settings))
    }
}

zed::register_extension!(EffectTsrsExtension);
