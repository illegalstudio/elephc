//! Purpose:
//! Compiles one hosted extension from source into a static archive Elephc can
//! link, and records the PHP surface it registers.
//!
//! Called from:
//! - `crate::php_ext::install` once the source tree and the `php-src` package
//!   (headers + `libelephc_zend.a`) are in place.
//!
//! Key details:
//! - What to compile comes from evaluating the extension's own `config.m4`
//!   (`config_m4`), exactly as phpize would feed it to configure; the defines it
//!   decides become the `config.h` every PECL source includes via
//!   `HAVE_CONFIG_H`.
//! - `COMPILE_DL_<EXT>` is deliberately never defined. It is what makes an
//!   extension emit `get_module()`, and two hosted extensions would then
//!   collide on that symbol; the module entry is reached through a generated
//!   accessor named after the extension instead.
//! - C and C++ units take their own `-std`: an extension's `-std=c++17` must not
//!   reach its C sources, where clang rejects it.
//! - The surface is taken by *running* the extension: a small introspector links
//!   the archive against the engine, starts the module and prints what it
//!   registered. That needs a binary the host can run, so an extension is built
//!   for the host target only.
//! - Before that, the archive's undefined symbols go through `admission`, so an
//!   engine-hook extension is refused instead of linking cleanly and never
//!   running.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::codegen_support::platform::{Platform, Target};
use crate::native_deps::{run_checked, NativeError, NativeErrorKind, NativeToolchain};

use super::admission::{self, CSymbol, Verdict};
use super::config_m4::{self, ExtensionConfig};
use super::surface::ExtensionSurface;

/// Bumped whenever this module changes what it produces, so existing artifacts
/// built by older logic are not reused. Part of every artifact's cache key.
pub const BUILD_REVISION: u32 = 1;

/// What one build needs.
pub struct BuildInputs<'a> {
    /// Manifest name of the extension.
    pub name: &'a str,
    /// The extension's source tree (the directory holding `config.m4`). Never
    /// written to: the build works on a copy.
    pub source_dir: &'a Path,
    /// Root of the materialized `php-src` package (`include/`, `lib/`).
    pub php_src: &'a Path,
    /// `PHP_VERSION` of those headers.
    pub php_version: &'a str,
    pub toolchain: &'a NativeToolchain,
    pub target: Target,
    /// Scratch directory, created and removed by the build.
    pub work_dir: &'a Path,
    /// Where `lib/`, `surface.json` and `build.json` are written.
    pub output_dir: &'a Path,
}

/// How to link an installed extension, stored beside it as `build.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecord {
    /// The name `PHP_NEW_EXTENSION` registered; the module entry is
    /// `<extension>_module_entry`.
    pub extension: String,
    /// Archive path relative to the artifact directory.
    pub archive: String,
    /// True when the archive holds C++ objects: the program must link the C++
    /// runtime.
    pub cxx: bool,
    /// System libraries named by `PHP_ADD_LIBRARY`.
    pub libraries: Vec<String>,
}

impl BuildRecord {
    /// The accessor the generated prelude calls to reach the module entry.
    pub fn module_accessor(&self) -> String {
        module_accessor(&self.extension)
    }
}

/// `elephc_php_ext_module_<extension>`, the generated accessor for a module entry.
pub fn module_accessor(extension: &str) -> String {
    format!("elephc_php_ext_module_{extension}")
}

fn build_error(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Build, message)
}

/// Builds the extension and returns its link record and surface. Both are also
/// written to `output_dir`.
pub fn build(inputs: &BuildInputs<'_>) -> Result<(BuildRecord, ExtensionSurface), NativeError> {
    if inputs.target != Target::detect_host() {
        return Err(build_error(format!(
            "hosted extension '{}' can only be installed for the host target ({}): its surface is \
             read by running it",
            inputs.name,
            Target::detect_host().as_str()
        )));
    }
    if inputs.work_dir.exists() {
        fs::remove_dir_all(inputs.work_dir)
            .map_err(|error| NativeError::io("clear hosted extension build directory", inputs.work_dir, error))?;
    }
    let source = inputs.work_dir.join("src");
    copy_tree(inputs.source_dir, &source)?;
    let result = build_in(inputs, &source);
    let _ = fs::remove_dir_all(inputs.work_dir);
    result
}

fn build_in(inputs: &BuildInputs<'_>, source: &Path) -> Result<(BuildRecord, ExtensionSurface), NativeError> {
    let config_path = source.join("config.m4");
    let script = fs::read_to_string(&config_path)
        .map_err(|error| NativeError::io("read extension config.m4", &config_path, error))?;
    let environment = config_m4::Environment {
        extension: inputs.name.to_string(),
        extension_dir: source.to_path_buf(),
        php_include_dir: inputs.php_src.join("include"),
        php_version: inputs.php_version.to_string(),
    };
    let config = config_m4::evaluate(&script, &environment).map_err(|message| {
        build_error(format!("hosted extension '{}': {message}", inputs.name))
    })?;
    if config.sources.is_empty() {
        return Err(build_error(format!("hosted extension '{}' lists no sources in config.m4", inputs.name)));
    }
    fs::write(source.join("config.h"), render_config_h(&config))
        .map_err(|error| NativeError::io("write extension config.h", &source.join("config.h"), error))?;

    let objects_dir = inputs.work_dir.join("obj");
    fs::create_dir_all(&objects_dir)
        .map_err(|error| NativeError::io("create extension object directory", &objects_dir, error))?;
    let includes = include_flags(&config, source, inputs.php_src);
    let mut objects = Vec::with_capacity(config.sources.len() + 1);
    for (index, unit) in config.sources.iter().enumerate() {
        let path = source.join(unit);
        if !path.is_file() {
            return Err(build_error(format!(
                "hosted extension '{}': config.m4 lists '{unit}', which is not in the source tree",
                inputs.name
            )));
        }
        let cxx = config_m4::is_cxx_source(unit);
        let object = objects_dir.join(object_name(index, unit));
        let driver = if cxx { cxx_driver(&inputs.toolchain.cc)? } else { inputs.toolchain.cc.clone() };
        let mut compile = inputs.toolchain.command(&driver);
        compile.args(unit_flags(&config, cxx)).args(&includes).arg("-c").arg(&path).arg("-o").arg(&object);
        run_checked(&mut compile, &format!("compile hosted extension '{}' unit {unit}", inputs.name))?;
        objects.push(object);
    }

    let accessor_source = objects_dir.join("elephc_module_accessor.c");
    fs::write(&accessor_source, render_module_accessor(&config.extension))
        .map_err(|error| NativeError::io("write module accessor", &accessor_source, error))?;
    let accessor_object = objects_dir.join("elephc_module_accessor.o");
    let mut compile = inputs.toolchain.command(&inputs.toolchain.cc);
    compile
        .args(["-O2", "-fPIC", "-std=gnu17"])
        .args(php_include_flags(inputs.php_src))
        .arg("-c")
        .arg(&accessor_source)
        .arg("-o")
        .arg(&accessor_object);
    run_checked(&mut compile, "compile hosted extension module accessor")?;
    objects.push(accessor_object);

    let archive_relative = format!("lib/libelephc_ext_{}.a", inputs.name);
    let archive = inputs.output_dir.join(&archive_relative);
    archive_objects(inputs.toolchain, &archive, &objects)?;
    admit(inputs, &archive)?;

    let record = BuildRecord {
        extension: config.extension.clone(),
        archive: archive_relative,
        cxx: config.cxx,
        libraries: config.libraries.clone(),
    };
    let surface = introspect(inputs, &record, &archive)?;
    write_json(&inputs.output_dir.join("build.json"), &serde_json::to_string_pretty(&record).expect("plain data"))?;
    write_json(&inputs.output_dir.join("surface.json"), &surface.to_json())?;
    Ok((record, surface))
}

/// The `config.h` phpize's configure would have written for this extension.
fn render_config_h(config: &ExtensionConfig) -> String {
    let mut text = String::from(
        "/* config.h — generated by Elephc from this extension's config.m4. */\n\
         #ifndef ELEPHC_PHP_EXT_CONFIG_H\n#define ELEPHC_PHP_EXT_CONFIG_H\n",
    );
    for (name, value) in &config.defines {
        text.push_str(&format!("#define {name} {value}\n"));
    }
    text.push_str("#endif\n");
    text
}

/// The accessor that hands the module entry to the generated prelude.
fn render_module_accessor(extension: &str) -> String {
    format!(
        "#include \"php.h\"\n\
         extern zend_module_entry {extension}_module_entry;\n\
         void *{accessor}(void) {{ return &{extension}_module_entry; }}\n",
        accessor = module_accessor(extension)
    )
}

fn php_include_flags(php_src: &Path) -> Vec<String> {
    crate::native_deps::recipes::php_src::include_dirs(&php_src.join("include"))
        .into_iter()
        .flat_map(|dir| ["-I".to_string(), dir.display().to_string()])
        .collect()
}

/// Include flags in phpize's order: the extension first, then its declared
/// include directories, then PHP's.
fn include_flags(config: &ExtensionConfig, source: &Path, php_src: &Path) -> Vec<String> {
    let mut flags = vec!["-I".to_string(), source.display().to_string()];
    for dir in &config.include_dirs {
        let path = Path::new(dir);
        let path = if path.is_absolute() { path.to_path_buf() } else { source.join(path) };
        flags.push("-I".to_string());
        flags.push(path.display().to_string());
    }
    flags.extend(php_include_flags(php_src));
    flags
}

/// Compiler flags for one unit. Third-party warnings are silenced: they are
/// upstream's to fix, and a wall of them would bury a real error.
fn unit_flags(config: &ExtensionConfig, cxx: bool) -> Vec<String> {
    let mut flags: Vec<String> = ["-O2", "-fPIC", "-w", "-DHAVE_CONFIG_H", "-DPHP_ATOM_INC"]
        .iter()
        .map(|flag| flag.to_string())
        .collect();
    let mut has_std = false;
    for flag in &config.cflags {
        if let Some(standard) = flag.strip_prefix("-std=") {
            let is_cxx_standard = standard.contains("++");
            if is_cxx_standard != cxx {
                continue;
            }
            has_std = true;
        }
        flags.push(flag.clone());
    }
    if !has_std {
        flags.push(if cxx { "-std=c++17" } else { "-std=gnu17" }.to_string());
    }
    flags
}

/// Unique object names: sources in different directories share stems.
fn object_name(index: usize, unit: &str) -> String {
    let stem = Path::new(unit).file_stem().and_then(|stem| stem.to_str()).unwrap_or("unit");
    format!("{index:04}-{stem}.o")
}

/// C driver names and the C++ driver that belongs with each.
const CXX_FOR: &[(&str, &str)] = &[("clang", "clang++"), ("gcc", "g++"), ("cc", "c++")];

/// Derives the C++ driver paired with a C compiler, keeping cross tuples
/// (`aarch64-linux-gnu-gcc`) and version suffixes (`clang-18`). An
/// unrecognised compiler is an error: falling back to the C driver would leave
/// the C++ runtime unresolved at link.
pub fn cxx_driver(cc: &Path) -> Result<PathBuf, NativeError> {
    let unpaired = |name: &str| {
        NativeError::new(
            NativeErrorKind::Toolchain,
            format!(
                "cannot derive a C++ driver from compiler '{name}'; a hosted extension with C++ \
                 sources needs one"
            ),
        )
    };
    let name = cc
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| unpaired(&cc.display().to_string()))?;
    let mut parts: Vec<&str> = name.split('-').collect();
    let position = parts
        .iter()
        .rposition(|part| CXX_FOR.iter().any(|(c, _)| c == part))
        .ok_or_else(|| unpaired(name))?;
    let (_, cxx) = CXX_FOR.iter().find(|(c, _)| *c == parts[position]).expect("matched above");
    parts[position] = cxx;
    Ok(cc.with_file_name(parts.join("-")))
}

/// The `nm` paired with the toolchain's archiver.
fn nm_for(ar: &Path) -> PathBuf {
    match ar.file_name().and_then(|name| name.to_str()) {
        Some(name) if name.ends_with("ar") => ar.with_file_name(format!("{}nm", &name[..name.len() - 2])),
        _ => PathBuf::from("nm"),
    }
}

fn archive_objects(toolchain: &NativeToolchain, archive: &Path, objects: &[PathBuf]) -> Result<(), NativeError> {
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent).map_err(|error| NativeError::io("create extension library directory", parent, error))?;
    }
    let mut create = toolchain.command(&toolchain.ar);
    create.arg("crs").arg(archive).args(objects);
    run_checked(&mut create, "archive hosted extension")?;
    let mut index = toolchain.command(&toolchain.ranlib);
    index.arg(archive);
    run_checked(&mut index, "index hosted extension archive")
}

/// Refuses an extension whose undefined symbols say it hooks the VM.
fn admit(inputs: &BuildInputs<'_>, archive: &Path) -> Result<(), NativeError> {
    let output = inputs
        .toolchain
        .command(&nm_for(&inputs.toolchain.ar))
        .arg("-u")
        .arg(archive)
        .output()
        .map_err(|error| build_error(format!("list hosted extension symbols: {error}")))?;
    let required: Vec<CSymbol> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .filter(|symbol| !symbol.ends_with(':'))
        .map(|symbol| CSymbol(symbol.to_string()))
        .collect();
    match admission::judge(&required, &BTreeSet::new()) {
        Verdict::Refuse { engine_hooks } => Err(build_error(format!(
            "hosted extension '{}' replaces part of the PHP engine ({}); a compiled program has no \
             engine loop to hook, so it would link and never run",
            inputs.name,
            engine_hooks.join(", ")
        ))),
        Verdict::Review { reason } => {
            eprintln!("warning: hosted extension '{}': {reason}", inputs.name);
            Ok(())
        }
        Verdict::Admit { .. } => Ok(()),
    }
}

/// Links and runs the introspector, returning what the module registered.
fn introspect(inputs: &BuildInputs<'_>, record: &BuildRecord, archive: &Path) -> Result<ExtensionSurface, NativeError> {
    let dir = inputs.work_dir.join("introspect");
    fs::create_dir_all(&dir).map_err(|error| NativeError::io("create introspector directory", &dir, error))?;
    let main_source = dir.join("main.c");
    fs::write(
        &main_source,
        format!(
            "#include <stdio.h>\n\
             void *{accessor}(void);\n\
             int elephc_php_ext_describe(void *module, FILE *out);\n\
             int main(void) {{ return elephc_php_ext_describe({accessor}(), stdout); }}\n",
            accessor = record.module_accessor()
        ),
    )
    .map_err(|error| NativeError::io("write introspector", &main_source, error))?;
    // Compiled as C on its own: handed to a C++ link driver as source, it
    // would be compiled as C++ and its references mangled.
    let main_object = dir.join("main.o");
    let mut compile = inputs.toolchain.command(&inputs.toolchain.cc);
    compile.args(["-O1", "-std=gnu17", "-c"]).arg(&main_source).arg("-o").arg(&main_object);
    run_checked(&mut compile, "compile hosted extension introspector")?;
    let binary = dir.join("describe");
    let driver = if record.cxx { cxx_driver(&inputs.toolchain.cc)? } else { inputs.toolchain.cc.clone() };
    let mut link = inputs.toolchain.command(&driver);
    link.arg(&main_object)
        .arg(archive)
        .arg(inputs.php_src.join(crate::native_deps::recipes::php_src::ENGINE_ARCHIVE))
        .args(record.libraries.iter().map(|library| format!("-l{library}")))
        .arg("-lm");
    if inputs.target.platform == Platform::Linux {
        link.args(["-lpthread", "-ldl"]);
    }
    link.arg("-o").arg(&binary);
    let output = link
        .output()
        .map_err(|error| build_error(format!("link hosted extension introspector: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let missing = undefined_symbols(&stderr);
        let detail = if missing.is_empty() {
            stderr.trim().to_string()
        } else {
            format!(
                "it needs symbols neither it nor Elephc's Zend engine provides: {}",
                missing.join(", ")
            )
        };
        return Err(build_error(format!("hosted extension '{}' does not link: {detail}", inputs.name)));
    }
    let run = Command::new(&binary)
        .env_clear()
        .output()
        .map_err(|error| build_error(format!("run hosted extension introspector: {error}")))?;
    if !run.status.success() {
        return Err(build_error(format!(
            "hosted extension '{}' failed to start: {}",
            inputs.name,
            String::from_utf8_lossy(&run.stderr).trim()
        )));
    }
    ExtensionSurface::from_json(&String::from_utf8_lossy(&run.stdout))
        .map_err(|message| build_error(format!("hosted extension '{}': {message}", inputs.name)))
}

/// Undefined symbol names from an ld64 or GNU ld failure, without duplicates.
fn undefined_symbols(stderr: &str) -> Vec<String> {
    let mut symbols = BTreeSet::new();
    for line in stderr.lines() {
        let line = line.trim();
        // ld64:  "_zend_foo", referenced from:
        if let Some(rest) = line.strip_prefix('"') {
            if let Some((symbol, tail)) = rest.split_once('"') {
                if tail.contains("referenced from") {
                    symbols.insert(symbol.strip_prefix('_').unwrap_or(symbol).to_string());
                }
            }
        }
        // GNU ld: undefined reference to `zend_foo'
        if let Some(position) = line.find("undefined reference to ") {
            let symbol = line[position + "undefined reference to ".len()..]
                .trim_matches(|c| c == '`' || c == '\'' || c == '"');
            symbols.insert(symbol.to_string());
        }
    }
    symbols.into_iter().collect()
}

fn write_json(path: &Path, text: &str) -> Result<(), NativeError> {
    fs::write(path, text).map_err(|error| NativeError::io("write hosted extension record", path, error))
}

/// Copies a source tree, skipping build leftovers a phpize run may have left.
fn copy_tree(from: &Path, to: &Path) -> Result<(), NativeError> {
    fs::create_dir_all(to).map_err(|error| NativeError::io("create extension source copy", to, error))?;
    let entries = fs::read_dir(from).map_err(|error| NativeError::io("read extension source", from, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| NativeError::io("read extension source entry", from, error))?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | ".libs" | "modules" | "autom4te.cache")) {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| NativeError::io("inspect extension source", &path, error))?;
        if file_type.is_dir() {
            copy_tree(&path, &to.join(&name))?;
        } else if file_type.is_file() {
            fs::copy(&path, to.join(&name)).map_err(|error| NativeError::io("copy extension source", &path, error))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(cflags: &[&str]) -> ExtensionConfig {
        ExtensionConfig {
            extension: "demo".into(),
            sources: vec!["demo.c".into()],
            cflags: cflags.iter().map(|flag| flag.to_string()).collect(),
            ..ExtensionConfig::default()
        }
    }

    /// simdjson passes `-std=c++17` for its C++ units; clang rejects that flag
    /// on a C unit, so it must only reach C++ ones.
    #[test]
    fn a_cxx_standard_never_reaches_a_c_unit() {
        let config = config(&["-std=c++17", "-DX=1"]);
        let c = unit_flags(&config, false);
        assert!(!c.iter().any(|flag| flag == "-std=c++17"));
        assert!(c.contains(&"-std=gnu17".to_string()), "C units get the default C standard");
        assert!(c.contains(&"-DX=1".to_string()));
        let cxx = unit_flags(&config, true);
        assert!(cxx.contains(&"-std=c++17".to_string()));
        assert_eq!(cxx.iter().filter(|flag| flag.starts_with("-std=")).count(), 1);
    }

    /// COMPILE_DL_<EXT> would make the extension emit get_module(), which two
    /// hosted extensions would both define.
    #[test]
    fn never_defines_compile_dl() {
        let flags = unit_flags(&config(&[]), false);
        assert!(!flags.iter().any(|flag| flag.contains("COMPILE_DL")));
        assert!(flags.contains(&"-DHAVE_CONFIG_H".to_string()));
    }

    #[test]
    fn config_h_carries_every_define() {
        let mut config = config(&[]);
        config.defines.insert("HAVE_DEMO".into(), "1".into());
        config.defines.insert("APC_MMAP".into(), "1".into());
        let text = render_config_h(&config);
        assert!(text.contains("#define HAVE_DEMO 1"));
        assert!(text.contains("#define APC_MMAP 1"));
    }

    #[test]
    fn the_accessor_reaches_the_module_entry() {
        let text = render_module_accessor("simdjson");
        assert!(text.contains("extern zend_module_entry simdjson_module_entry;"));
        assert!(text.contains("void *elephc_php_ext_module_simdjson(void)"));
    }

    #[test]
    fn derives_cxx_drivers() {
        assert_eq!(cxx_driver(Path::new("/usr/bin/clang")).unwrap(), PathBuf::from("/usr/bin/clang++"));
        assert_eq!(cxx_driver(Path::new("/usr/bin/cc")).unwrap(), PathBuf::from("/usr/bin/c++"));
        assert_eq!(
            cxx_driver(Path::new("/opt/x/aarch64-linux-gnu-gcc")).unwrap(),
            PathBuf::from("/opt/x/aarch64-linux-gnu-g++")
        );
        assert_eq!(cxx_driver(Path::new("clang-18")).unwrap(), PathBuf::from("clang++-18"));
        assert!(cxx_driver(Path::new("/usr/bin/tcc")).is_err());
    }

    #[test]
    fn pairs_nm_with_the_archiver() {
        assert_eq!(nm_for(Path::new("/usr/bin/ar")), PathBuf::from("/usr/bin/nm"));
        assert_eq!(nm_for(Path::new("/x/aarch64-linux-gnu-ar")), PathBuf::from("/x/aarch64-linux-gnu-nm"));
    }

    #[test]
    fn same_stem_sources_get_distinct_objects() {
        assert_ne!(object_name(0, "src/php/hash.c"), object_name(1, "src/ds/hash.c"));
    }

    /// A link failure is reported as the symbols that are missing, in C
    /// spelling, whichever linker produced it.
    #[test]
    fn reads_undefined_symbols_from_both_linkers() {
        let ld64 = "Undefined symbols for architecture arm64:\n  \"_php_session_register_serializer\", referenced from:\n      _zm_startup_msgpack in libelephc_ext_msgpack.a[2](0002-msgpack.o)\nld: symbol(s) not found";
        assert_eq!(undefined_symbols(ld64), vec!["php_session_register_serializer"]);
        let gnu = "/usr/bin/ld: msgpack.o: in function `zm_startup_msgpack':\nmsgpack.c:(.text+0x1c): undefined reference to `php_session_register_serializer'";
        assert_eq!(undefined_symbols(gnu), vec!["php_session_register_serializer"]);
    }
}
