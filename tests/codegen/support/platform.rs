use super::*;

pub(crate) fn target() -> Target {
    *TEST_TARGET.get_or_init(|| {
        std::env::var("ELEPHC_TEST_TARGET")
            .ok()
            .map(|value| Target::parse(&value).expect("invalid ELEPHC_TEST_TARGET"))
            .unwrap_or_else(Target::detect_host)
    })
}

pub(crate) fn get_sdk_path() -> &'static str {
    SDK_PATH.get_or_init(|| {
        Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    })
}

pub(crate) fn get_sdk_version() -> &'static str {
    SDK_VERSION.get_or_init(|| {
        match Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-version"])
            .output()
        {
            Ok(output) => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if version.is_empty() {
                    "15.0".to_string()
                } else {
                    version
                }
            }
            Err(_) => "15.0".to_string(),
        }
    })
}

/// Get the assembler command for the current platform.
pub(crate) fn assembler_cmd() -> &'static str {
    target().assembler_cmd()
}

/// Get the linker/gcc command for the current platform.
pub(crate) fn gcc_cmd() -> &'static str {
    target().linker_cmd()
}

pub(crate) fn default_link_paths() -> Vec<String> {
    let mut paths = Vec::new();
    match target().platform {
        Platform::MacOS => {
            for candidate in ["/opt/homebrew/lib", "/usr/local/lib"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
        Platform::Linux => {
            for candidate in ["/usr/aarch64-linux-gnu/lib", "/usr/lib/aarch64-linux-gnu"] {
                if std::path::Path::new(candidate).exists() {
                    paths.push(candidate.to_string());
                }
            }
        }
    }
    paths
}

pub(crate) fn effective_link_libs(extra_link_libs: &[String]) -> Vec<&str> {
    extra_link_libs
        .iter()
        .map(String::as_str)
        .filter(|lib| *lib != "System")
        .collect()
}

pub(crate) fn qemu_sysroot() -> Option<&'static str> {
    QEMU_SYSROOT
        .get_or_init(|| match target().platform {
            Platform::Linux => {
                let compiler = gcc_cmd();
                if let Ok(output) = Command::new(compiler).arg("-print-sysroot").output() {
                    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !sysroot.is_empty()
                        && sysroot != "/"
                        && std::path::Path::new(&sysroot).exists()
                    {
                        return Some(sysroot);
                    }
                }
                for candidate in ["/usr/aarch64-linux-gnu", "/usr/local/aarch64-linux-gnu"] {
                    if std::path::Path::new(candidate)
                        .join("lib/ld-linux-aarch64.so.1")
                        .exists()
                        || std::path::Path::new(candidate)
                            .join("lib64/ld-linux-aarch64.so.1")
                            .exists()
                    {
                        return Some(candidate.to_string());
                    }
                }
                None
            }
            Platform::MacOS => None,
        })
        .as_deref()
}

#[test]
fn test_effective_link_libs_ignores_system() {
    let libs = vec!["System".to_string(), "crypto".to_string()];
    assert_eq!(effective_link_libs(&libs), vec!["crypto"]);
}
