//! Purpose:
//! Builds a static HTML Tidy 5.8.0 archive from the upstream C sources for the
//! `elephc-tidy` bridge, and retains libtidy's four public headers.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for the `libtidy` recipe revision 1.
//!
//! Key details:
//! - Upstream is CMake-only, but this repository's managed toolchain carries just
//!   `cc`/`ar`/`ranlib`. The reviewed recipe compiles the exact CMake `CFILES` list
//!   directly, in source order, with the same release macros and no generated files.
//! - `SUPPORT_LOCALIZATIONS=0` and `SUPPORT_CONSOLE_APP=0` keep the library free of
//!   message catalogs and console-only code; the bridge never uses either. No
//!   `TIDY_ENABLE_CONFIG_FILES` is defined, so `tidyCreate()` cannot silently read a
//!   host `/etc/tidy.conf` or `~/.tidyrc`. PHP supplies configuration explicitly.
//! - `-O2 -fPIC` plus `-ffile-prefix-map=` entries scrub the per-run staging paths
//!   from `__FILE__` so the archive is reproducible; no `-g` is emitted.
//! - There are no catalogued dependencies: libtidy needs only the C library.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;
use super::util::copy_regular;

/// The exact CMake `CFILES` list from tidy-html5 5.8.0's `CMakeLists.txt`, in order.
const SOURCE_FILES: &[&str] = &[
    "access",
    "attrs",
    "istack",
    "parser",
    "tags",
    "entities",
    "lexer",
    "pprint",
    "charsets",
    "clean",
    "message",
    "config",
    "alloc",
    "attrdict",
    "buffio",
    "fileio",
    "streamio",
    "tagask",
    "tmbstr",
    "utf8",
    "tidylib",
    "mappedio",
    "gdoc",
    "language",
    "messageobj",
    "sprtf",
];

/// The release date baked into libtidy's `tidyReleaseDate()` by the upstream build.
const RELEASE_DATE: &str = "2021/07/10";

/// The optimization and code-model flags every translation unit is compiled with.
const BASE_CFLAGS: &[&str] = &["-O2", "-fPIC"];

/// Builds one `-ffile-prefix-map=<old>=<new>` argument without lossy path conversion.
fn prefix_map(old: &Path, new: &str) -> OsString {
    let mut flag = OsString::from("-ffile-prefix-map=");
    flag.push(old.as_os_str());
    flag.push("=");
    flag.push(new);
    flag
}

/// Builds libtidy and stages `lib/libtidy.a` plus the four public headers.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let include = request.staging_prefix.join("include");
    let library = request.staging_prefix.join("lib");
    fs::create_dir_all(&build)
        .map_err(|error| NativeError::io("create libtidy build directory", &build, error))?;
    fs::create_dir_all(&include)
        .map_err(|error| NativeError::io("create libtidy include directory", &include, error))?;
    fs::create_dir_all(&library)
        .map_err(|error| NativeError::io("create libtidy library directory", &library, error))?;

    let prefix_maps = [
        prefix_map(request.source, &format!("tidy-html5-{}", request.version.version)),
        prefix_map(&build, "build"),
    ];
    let version_flag = format!("-DLIBTIDY_VERSION=\"{}\"", request.version.version);
    let date_flag = format!("-DRELEASE_DATE=\"{RELEASE_DATE}\"");

    let mut objects = Vec::with_capacity(SOURCE_FILES.len());
    for name in SOURCE_FILES {
        let source = request.source.join("src").join(format!("{name}.c"));
        let object = build.join(format!("{name}.o"));
        let mut command = request.toolchain.command(&request.toolchain.cc);
        command
            .args(BASE_CFLAGS)
            .args(&prefix_maps)
            .arg("-I")
            .arg(request.source.join("include"))
            .arg("-I")
            .arg(request.source.join("src"))
            .arg(&version_flag)
            .arg(&date_flag)
            .arg("-DSUPPORT_LOCALIZATIONS=0")
            .arg("-DSUPPORT_CONSOLE_APP=0")
            .arg("-D_REENTRANT")
            .arg("-c")
            .arg(&source)
            .arg("-o")
            .arg(&object);
        run_checked(&mut command, "compile trusted libtidy source")?;
        objects.push(object);
    }

    let archive = library.join("libtidy.a");
    let mut archive_command = request.toolchain.command(&request.toolchain.ar);
    archive_command.arg("crs").arg(&archive);
    for object in &objects {
        archive_command.arg(object);
    }
    run_checked(&mut archive_command, "archive trusted libtidy static library")?;

    let mut ranlib = request.toolchain.command(&request.toolchain.ranlib);
    ranlib.arg(&archive);
    run_checked(&mut ranlib, "index trusted libtidy static library")?;

    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate trusted libtidy static archive")?;

    for header in request.version.retained_headers {
        let Some(name) = header.strip_prefix("include/") else {
            continue;
        };
        copy_regular(
            "libtidy",
            &request.source.join("include").join(name),
            &include.join(name),
        )?;
    }
    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove trusted libtidy build tree", &build, error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural guards for the reviewed libtidy recipe.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These assertions keep the source list, release macros, and optional-feature
    //!   switches aligned with upstream 5.8.0 without requiring a network build.

    use super::*;

    /// The recipe compiles every upstream CMake library source exactly once.
    #[test]
    fn source_list_matches_upstream_library_order() {
        assert_eq!(SOURCE_FILES.len(), 26);
        assert_eq!(SOURCE_FILES.first(), Some(&"access"));
        assert_eq!(SOURCE_FILES.last(), Some(&"sprtf"));
        assert!(SOURCE_FILES.contains(&"tidylib"));
        assert!(SOURCE_FILES.contains(&"config"));
    }

    /// The release date and optional-feature switches are explicit and stable.
    #[test]
    fn release_and_optional_features_are_pinned() {
        assert_eq!(RELEASE_DATE, "2021/07/10");
        assert!(!SOURCE_FILES.contains(&"tidy"));
        assert!(!BASE_CFLAGS.contains(&"-g"));
    }

    /// The prefix maps rename both staging trees to fixed names without path separators.
    #[test]
    fn prefix_maps_scrub_staging_paths() {
        let source = Path::new("/home/builder/.cache/elephc/native/artifacts/libtidy/.stage.1/.source");
        let build = Path::new("/home/builder/.cache/elephc/native/artifacts/libtidy/.stage.1/build");
        let maps = [
            prefix_map(source, "tidy-html5-5.8.0"),
            prefix_map(build, "build"),
        ];
        assert_eq!(
            maps[0].to_str().unwrap(),
            "-ffile-prefix-map=/home/builder/.cache/elephc/native/artifacts/libtidy/.stage.1/.source=tidy-html5-5.8.0"
        );
        assert_eq!(
            maps[1].to_str().unwrap(),
            "-ffile-prefix-map=/home/builder/.cache/elephc/native/artifacts/libtidy/.stage.1/build=build"
        );
    }

}
