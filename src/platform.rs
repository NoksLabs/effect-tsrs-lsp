use zed_extension_api::{self as zed, Result};

const PACKAGE_SCOPE: &str = "@tsrs/";
const BUNDLE_SCOPE: &str = "@effect/tsgo-";

/// The packages that can provide a tsrs binary for this machine.
pub struct Platform {
    /// Candidate `@tsrs/<platform>` package names, most preferred first.
    pub packages: Vec<String>,
    /// The `@effect/tsgo-<platform>` package, when it bundles an Effect-patched tsrs for
    /// this machine (darwin-arm64, linux-x64 and linux-arm64 with glibc).
    pub bundle: Option<String>,
    pub exe_name: &'static str,
}

impl Platform {
    pub fn current() -> Result<Self> {
        let (os, arch) = zed::current_platform();
        let arch = match arch {
            zed::Architecture::Aarch64 => "arm64",
            zed::Architecture::X8664 => "x64",
            zed::Architecture::X86 => return Err("tsrs does not ship a 32-bit x86 binary".into()),
        };
        // Linux packages carry a libc suffix; which one exists depends on what the
        // package manager installed. The bundled build only targets glibc, as does Zed.
        let (names, bundle, exe_name) = match os {
            zed::Os::Mac => (
                vec![format!("darwin-{arch}")],
                (arch == "arm64").then(|| format!("darwin-{arch}")),
                "tsrs",
            ),
            zed::Os::Linux => (
                vec![format!("linux-{arch}-gnu"), format!("linux-{arch}-musl")],
                Some(format!("linux-{arch}")),
                "tsrs",
            ),
            zed::Os::Windows => (vec![format!("win32-{arch}")], None, "tsrs.exe"),
        };
        Ok(Self {
            packages: names
                .into_iter()
                .map(|name| format!("{PACKAGE_SCOPE}{name}"))
                .collect(),
            bundle: bundle.map(|name| format!("{BUNDLE_SCOPE}{name}")),
            exe_name,
        })
    }
}
