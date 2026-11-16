//! Purpose:
//! Builds pinned static libmagic and embeds its complete compiled magic database.
//!
//! Called from:
//! - The curated native package dispatcher for libmagic recipe revision 1.
//!
//! Key details:
//! - A host tool compiles the database for cross targets; libmagic swaps its endianness.
//! - Optional compression and sandbox libraries are disabled explicitly.
//! - Deployed programs need neither a system library nor a database file.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use crate::codegen_support::platform::Target;
use super::super::{error::NativeError, recipe::RecipeRequest, toolchain::run_checked};
use super::util::copy_regular;

const FLAGS: &[&str] = &[
    "--disable-shared", "--enable-static", "--disable-elf", "--disable-elf-core", "--disable-zlib", "--disable-bzlib",
    "--disable-xzlib", "--disable-zstdlib", "--disable-lzlib", "--disable-lrziplib",
    "--disable-libseccomp", "CFLAGS=-O2 -fPIC",
    // Fileinfo does not launch external decompressors, including on iOS.
    "ac_cv_func_fork=no", "ac_cv_func_vfork=no", "ac_cv_func_posix_spawnp=no",
];

/// Builds the host database compiler when needed, then the target archives and database.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("build");
    let library = request.staging_prefix.join("lib");
    let include = request.staging_prefix.join("include");
    for dir in [&build, &library, &include] {
        fs::create_dir_all(dir).map_err(|e| NativeError::io("create libmagic directory", dir, e))?;
    }
    let cross = request.target != Target::detect_host();
    let host = request.staging_prefix.join("host-build");
    if cross {
        fs::create_dir_all(&host).map_err(|e| NativeError::io("create libmagic host directory", &host, e))?;
        let mut configure = Command::new("/bin/sh");
        configure.current_dir(&host).arg(request.source.join("configure")).args(FLAGS);
        // Cross compiler variables must not leak into the executable database compiler.
        for variable in ["CC", "CXX", "AR", "RANLIB", "CFLAGS", "CPPFLAGS", "LDFLAGS", "SDKROOT"] {
            configure.env_remove(variable);
        }
        run_checked(&mut configure, "configure libmagic host database compiler")?;
        // Named automake targets skip BUILT_SOURCES, so generate the public header first.
        let mut header = Command::new("make");
        header.current_dir(&host).args(["-C", "src", "magic.h"]);
        run_checked(&mut header, "generate libmagic host header")?;
        let mut make = Command::new("make");
        make.current_dir(&host).args(["-C", "src", "file"]);
        run_checked(&mut make, "build libmagic host database compiler")?;
    }
    let mut configure = request.toolchain.command(Path::new("/bin/sh"));
    configure.current_dir(&build).arg(request.source.join("configure")).args(FLAGS);
    if cross { configure.arg(format!("--host={}", request.toolchain.autoconf_host())); }
    run_checked(&mut configure, "configure static libmagic")?;
    let mut header = request.toolchain.command(Path::new("make"));
    header.current_dir(&build).args(["-C", "src", "magic.h"]);
    run_checked(&mut header, "generate libmagic target header")?;
    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).args(["-C", "src", if cross { "libmagic.la" } else { "file" }]);
    run_checked(&mut make, "build static libmagic")?;
    let mut database = request.toolchain.command(Path::new("make"));
    database.current_dir(&build).args(["-C", "magic", "magic.mgc"]);
    if cross { database.arg(format!("FILE_COMPILE={}", host.join("src/file").display())); }
    run_checked(&mut database, "compile embedded magic database")?;
    copy_regular("libmagic", &build.join("src/.libs/libmagic.a"), &library.join("libmagic.a"))?;
    copy_regular("libmagic", &build.join("src/magic.h"), &include.join("magic.h"))?;
    embed_database(request, &build.join("magic/magic.mgc"), &build, &include, &library)?;
    fs::remove_dir_all(&build).map_err(|e| NativeError::io("remove libmagic build directory", &build, e))?;
    if cross { fs::remove_dir_all(&host).map_err(|e| NativeError::io("remove libmagic host directory", &host, e))?; }
    Ok(())
}

/// Archives an aligned C array and loader so the database travels with the target binary.
fn embed_database(request: &RecipeRequest<'_>, database: &Path, build: &Path, include: &Path, library: &Path) -> Result<(), NativeError> {
    let bytes = fs::read(database).map_err(|e| NativeError::io("read compiled magic database", database, e))?;
    let source = build.join("database.c");
    let object = build.join("database.o");
    let mut file = fs::File::create(&source).map_err(|e| NativeError::io("create embedded database source", &source, e))?;
    let write = || -> std::io::Result<()> {
        writeln!(file, "#include <magic.h>\n#include <stddef.h>\nstatic const unsigned char database[] __attribute__((aligned(8))) = {{")?;
        for chunk in bytes.chunks(32) {
            for byte in chunk { write!(file, "{byte},")?; }
            writeln!(file)?;
        }
        writeln!(file, "}};\nint elephc_magic_load_default(magic_t m) {{ void *p = (void *)database; size_t n = sizeof(database); return magic_load_buffers(m, &p, &n, 1); }}")
    };
    let mut write = write;
    write().map_err(|e| NativeError::io("write embedded database source", &source, e))?;
    drop(file);
    let mut cc = request.toolchain.command(&request.toolchain.cc);
    cc.args(["-O2", "-fPIC", "-I"]).arg(include).arg("-c").arg(&source).arg("-o").arg(&object);
    run_checked(&mut cc, "compile embedded magic database")?;
    let mut ar = request.toolchain.command(&request.toolchain.ar);
    ar.arg("crs").arg(library.join("libelephc_magic_database.a")).arg(&object);
    run_checked(&mut ar, "archive embedded magic database")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Prevents accidental host-library dependencies or a shared-only artifact.
    #[test]
    fn fileinfo_recipe_is_static_and_self_contained() {
        assert!(FLAGS.contains(&"--enable-static"));
        for flag in ["--disable-zlib", "--disable-bzlib", "--disable-xzlib", "--disable-zstdlib", "--disable-libseccomp"] {
            assert!(FLAGS.contains(&flag));
        }
    }
}
