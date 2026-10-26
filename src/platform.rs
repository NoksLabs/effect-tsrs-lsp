use zed_extension_api::{self as zed, Result};

const PACKAGE_SCOPE: &str = "@tsrs/";

/// The `@tsrs/<platform>` packages that can run on this machine.
pub struct Platform {
    /// Candidate package names, most preferred first.
    pub packages: Vec<String>,
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
        // package manager installed.
        let (names, exe_name) = match os {
            zed::Os::Mac => (vec![format!("darwin-{arch}")], "tsrs"),
            zed::Os::Linux => (
                vec![format!("linux-{arch}-gnu"), format!("linux-{arch}-musl")],
                "tsrs",
            ),
            zed::Os::Windows => (vec![format!("win32-{arch}")], "tsrs.exe"),
        };
        Ok(Self {
            packages: names
                .into_iter()
                .map(|name| format!("{PACKAGE_SCOPE}{name}"))
                .collect(),
            exe_name,
        })
    }
}
