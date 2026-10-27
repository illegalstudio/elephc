//! Purpose:
//! Evaluates an extension's `config.m4` — the autoconf input `phpize` would turn
//! into a `configure` script — to learn what to compile: sources, C++ or not,
//! compiler flags, `config.h` defines, include directories and dependencies.
//!
//! Called from:
//! - `crate::php_ext::build` before compiling a hosted extension.
//!
//! Key details:
//! - `config.m4` is a shell program wrapped in m4 macros, not a declaration
//!   list. Sources live in variables (`apc_sources="…"`), defines sit inside
//!   `if test` branches and `AS_VAR_IF` arms, and options have defaults declared
//!   by `PHP_ARG_ENABLE`. Reading every `AC_DEFINE` regardless of branch would
//!   turn on mutually exclusive options (APCu declares four lock backends), so
//!   this is a small evaluator for the subset extensions actually use.
//! - No `phpize`, autoconf or shell is run. The extension being built is
//!   enabled; every other option takes the default its `PHP_ARG_*` declares.
//! - Autoconf probes (`AC_RUN_IFELSE`, `AC_COMPILE_IFELSE`, …) are answered as a
//!   modern POSIX system would, since every supported target is one. Function,
//!   library and header probes are not: platforms differ there (`mremap` is
//!   Linux-only, `sys/inotify.h` too), so they run autoconf's own test with the
//!   target toolchain. `AC_CHECK_FUNC(S)` and `AC_CHECK_LIB`/`PHP_CHECK_LIBRARY`
//!   link a program calling the function ([`Environment::function_probe`]);
//!   `AC_CHECK_HEADER(S)` compiles a unit including the header
//!   ([`Environment::header_probe`]). A failed library probe takes its failure
//!   branch, which is where an extension reports a missing dependency with
//!   `AC_MSG_ERROR` — surfaced verbatim. `AC_CHECK_LIB` without a success action
//!   links the library and defines `HAVE_LIB<LIB>`, as autoconf's default does,
//!   and `AC_CHECK_SIZEOF` answers the LP64 size every supported target shares.
//! - Anything the evaluator does not model is ignored rather than guessed, and
//!   recorded in [`ExtensionConfig::notes`] so a failed build can be explained.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// A required or optional dependency on another PHP extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionDependency {
    pub name: String,
    pub optional: bool,
}

/// What evaluating `config.m4` decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionConfig {
    /// The name `PHP_NEW_EXTENSION` registers.
    pub extension: String,
    /// Translation units, relative to the extension directory.
    pub sources: Vec<String>,
    /// Extra compiler flags, in declaration order.
    pub cflags: Vec<String>,
    /// Extra flags for C++ units only (`CXXFLAGS`), in declaration order.
    pub cxxflags: Vec<String>,
    /// True when any unit must be compiled as C++.
    pub cxx: bool,
    /// `config.h` defines, by name.
    pub defines: BTreeMap<String, String>,
    /// Include directories, relative to the extension directory or absolute.
    pub include_dirs: Vec<String>,
    pub dependencies: Vec<ExtensionDependency>,
    /// System libraries the extension links (`PHP_ADD_LIBRARY`, `-l` in `LIBS`).
    pub libraries: Vec<String>,
    /// Library search directories (`PHP_ADD_LIBRARY_WITH_PATH`, `-L` in `LIBS`/`LDFLAGS`).
    pub library_dirs: Vec<String>,
    /// Constructs that were not modelled, kept for diagnostics.
    pub notes: Vec<String>,
}

/// Answers whether a C function links on the build's target, given the libraries and library
/// directories the script has added so far: autoconf's `AC_CHECK_FUNC` test.
#[derive(Clone)]
pub struct FunctionProbe(std::rc::Rc<dyn Fn(&str, &[String], &[String]) -> bool>);

impl FunctionProbe {
    pub fn new(probe: impl Fn(&str, &[String], &[String]) -> bool + 'static) -> Self {
        Self(std::rc::Rc::new(probe))
    }

    /// Whether a program calling `function` links with `libraries` found in `library_dirs`.
    pub fn links(&self, function: &str, libraries: &[String], library_dirs: &[String]) -> bool {
        (self.0)(function, libraries, library_dirs)
    }
}

impl std::fmt::Debug for FunctionProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FunctionProbe")
    }
}

/// Answers whether a C header compiles on the build's target with the include directories the
/// script has set up so far: autoconf's `AC_CHECK_HEADER` test.
#[derive(Clone)]
pub struct HeaderProbe(std::rc::Rc<dyn Fn(&str, &[String]) -> bool>);

impl HeaderProbe {
    pub fn new(probe: impl Fn(&str, &[String]) -> bool + 'static) -> Self {
        Self(std::rc::Rc::new(probe))
    }

    /// Whether a unit including `header` compiles with `include_dirs` on the search path.
    pub fn compiles(&self, header: &str, include_dirs: &[String]) -> bool {
        (self.0)(header, include_dirs)
    }
}

impl std::fmt::Debug for HeaderProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HeaderProbe")
    }
}

/// Facts about the build the script may query.
#[derive(Clone, Debug)]
pub struct Environment {
    /// Manifest name of the extension being built; its `PHP_ARG_*` is enabled.
    pub extension: String,
    /// Absolute path of the extension source directory.
    pub extension_dir: PathBuf,
    /// Absolute path of the staged PHP `include/` directory.
    pub php_include_dir: PathBuf,
    /// `PHP_VERSION` of the headers, e.g. `8.5.6`.
    pub php_version: String,
    /// Links probe programs for `AC_CHECK_FUNC(S)` and `AC_CHECK_LIB`/`PHP_CHECK_LIBRARY`.
    pub function_probe: FunctionProbe,
    /// Compiles probe units for `AC_CHECK_HEADER(S)`.
    pub header_probe: HeaderProbe,
}

/// Evaluates `text` and returns the extension's build configuration, or the
/// message of the `AC_MSG_ERROR` the script reached.
pub fn evaluate(text: &str, env: &Environment) -> Result<ExtensionConfig, String> {
    let mut interpreter = Interpreter::new(env);
    interpreter.run(&strip_dnl(text), 0)?;
    interpreter.adopt_global_flags();
    let mut config = interpreter.config;
    if config.extension.is_empty() {
        return Err("config.m4 never reached PHP_NEW_EXTENSION".to_string());
    }
    if config
        .sources
        .iter()
        .any(|source| is_cxx_source(source))
    {
        config.cxx = true;
    }
    Ok(config)
}

/// True for the source suffixes phpize compiles with the C++ driver.
pub fn is_cxx_source(source: &str) -> bool {
    [".cpp", ".cc", ".cxx", ".c++"].iter().any(|suffix| source.ends_with(suffix))
}

/// Removes m4 `dnl` comments (to end of line) outside `[ ]` quotes.
fn strip_dnl(text: &str) -> String {
    // Scanned as bytes: the delimiters are ASCII, and a cut from `dnl` to the
    // newline never splits a UTF-8 character, so the rest is copied verbatim.
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'[' {
            depth += 1;
        } else if c == b']' {
            depth = depth.saturating_sub(1);
        }
        let word_start = i == 0 || !is_word_byte(bytes[i - 1]);
        if depth == 0
            && word_start
            && bytes[i..].starts_with(b"dnl")
            && bytes.get(i + 3).is_none_or(|b| !is_word_byte(*b))
        {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Whether a byte can be part of an identifier-like word (ASCII letter, digit or underscore).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The size of a standard C type on every supported target (all LP64), for `AC_CHECK_SIZEOF`;
/// `None` for a type whose size differs between them (`long double`) or that is not standard.
fn lp64_sizeof(ty: &str) -> Option<u32> {
    let normalized = ty.split_whitespace().collect::<Vec<_>>().join(" ").replace(" *", "*");
    Some(match normalized.as_str() {
        "char" | "unsigned char" | "signed char" | "int8_t" | "uint8_t" | "_Bool" | "bool" => 1,
        "short" | "unsigned short" | "short int" | "int16_t" | "uint16_t" => 2,
        "int" | "unsigned int" | "unsigned" | "int32_t" | "uint32_t" | "float" => 4,
        "long" | "unsigned long" | "long int" | "long long" | "unsigned long long" | "long long int"
        | "int64_t" | "uint64_t" | "size_t" | "ssize_t" | "off_t" | "ptrdiff_t" | "intptr_t"
        | "uintptr_t" | "intmax_t" | "uintmax_t" | "double" | "time_t" | "void*" | "char*" => 8,
        _ => return None,
    })
}

/// The identifier autoconf derives from a name for a `HAVE_`/`SIZEOF_` define: upper case, every
/// other byte than a letter or digit an underscore (`unsigned long` gives `UNSIGNED_LONG`).
fn shell_identifier(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect()
}

/// Whether phpize's m4 environment defines the macro `m4_ifdef` asks about: autoconf, libtool,
/// m4 and PHP's own macros are always there, so their `ifdef` takes the defined branch; anything
/// else (a third-party macro the extension did not ship) is undefined.
fn phpize_defines(name: &str) -> bool {
    let name = name.trim_start_matches('[').trim_end_matches(']');
    ["AC_", "AS_", "AH_", "AU_", "LT_", "PHP_", "m4_", "_AC_", "AX_CHECK_COMPILE_FLAG"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

struct Interpreter<'a> {
    env: &'a Environment,
    vars: HashMap<String, String>,
    config: ExtensionConfig,
    /// Function, library and header probe answers per name and link line (or include path), so a
    /// check repeated after `LIBS` or `CPPFLAGS` changes probes again.
    functions: HashMap<String, bool>,
}

impl<'a> Interpreter<'a> {
    /// Creates an interpreter for the extension environment, seeding the shell variables configure
    /// would have set (source and build directories, include directory, `ext_shared`, PHP version,
    /// compiler names and empty flags).
    fn new(env: &'a Environment) -> Self {
        let ext_dir = env.extension_dir.display().to_string();
        let mut vars = HashMap::new();
        for name in ["ext_srcdir", "ext_builddir", "abs_srcdir", "abs_builddir", "srcdir", "builddir"] {
            vars.insert(name.to_string(), ext_dir.clone());
        }
        vars.insert("phpincludedir".to_string(), env.php_include_dir.display().to_string());
        vars.insert("ext_shared".to_string(), "yes".to_string());
        vars.insert("PHP_CONFIG".to_string(), "php-config".to_string());
        vars.insert("PHP_VERSION".to_string(), env.php_version.clone());
        vars.insert("CC".to_string(), "cc".to_string());
        vars.insert("CXX".to_string(), "c++".to_string());
        vars.insert("GCC".to_string(), "yes".to_string());
        vars.insert("CFLAGS".to_string(), String::new());
        vars.insert("LIBS".to_string(), String::new());
        Self { env, vars, config: ExtensionConfig::default(), functions: HashMap::new() }
    }

    /// Whether a program calling `function` links with the libraries added so far, plus the
    /// `-l`/`-L` flags `LDFLAGS` and `LIBS` hold right now, as configure's link test reads them;
    /// a temporary `LIBS="$LIBS -lm"` around the check reaches the probe without being kept.
    /// A name that is not a C identifier never links.
    fn function_links(&mut self, function: &str) -> bool {
        self.function_links_with(function, "")
    }

    /// [`Self::function_links`] with `extra` link flags first, the way `AC_CHECK_LIB` puts the
    /// library it tests (and its other-libraries argument) ahead of `LIBS`.
    fn function_links_with(&mut self, function: &str, extra: &str) -> bool {
        let mut libraries = Vec::new();
        let mut library_dirs = Vec::new();
        let mut flags: Vec<String> = extra.split_whitespace().map(str::to_string).collect();
        flags.extend(self.config.library_dirs.iter().map(|dir| format!("-L{dir}")));
        flags.extend(self.config.libraries.iter().map(|library| format!("-l{library}")));
        for name in ["LDFLAGS", "LIBS"] {
            let line = self.vars.get(name).cloned().unwrap_or_default();
            flags.extend(line.split_whitespace().map(str::to_string));
        }
        for flag in &flags {
            let flag = flag.trim_matches('"');
            if let Some(library) = flag.strip_prefix("-l").filter(|l| !l.is_empty()) {
                if !libraries.iter().any(|known| known == library) {
                    libraries.push(library.to_string());
                }
            } else if let Some(dir) = flag.strip_prefix("-L").filter(|d| !d.is_empty()) {
                if !library_dirs.iter().any(|known| known == dir) {
                    library_dirs.push(dir.to_string());
                }
            }
        }
        let key = format!("{function}\0{}\0{}", libraries.join(" "), library_dirs.join(" "));
        if let Some(&found) = self.functions.get(&key) {
            return found;
        }
        let identifier = function.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
            && function.chars().all(|c| c == '_' || c.is_ascii_alphanumeric());
        let found = identifier && self.env.function_probe.links(function, &libraries, &library_dirs);
        self.functions.insert(key, found);
        found
    }

    /// Whether a unit including `header` compiles with the include directories added so far and
    /// the `-I` flags `CPPFLAGS` and `CFLAGS` hold right now; relative directories are taken from
    /// the extension directory, as the build compiles from there.
    fn header_compiles(&mut self, header: &str) -> bool {
        let mut dirs = Vec::new();
        let flags = ["CPPFLAGS", "CFLAGS"]
            .iter()
            .map(|name| self.vars.get(*name).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        let declared = self.config.include_dirs.iter().cloned();
        let from_flags = flags
            .split_whitespace()
            .filter_map(|flag| flag.trim_matches('"').strip_prefix("-I").filter(|d| !d.is_empty()).map(str::to_string));
        for dir in declared.chain(from_flags) {
            let dir = if Path::new(&dir).is_absolute() {
                dir
            } else {
                self.env.extension_dir.join(&dir).to_string_lossy().into_owned()
            };
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        let key = format!("header\0{header}\0{}", dirs.join(" "));
        if let Some(&found) = self.functions.get(&key) {
            return found;
        }
        let plausible = !header.is_empty()
            && header.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+'));
        let found = plausible && self.env.header_probe.compiles(header, &dirs);
        self.functions.insert(key, found);
        found
    }

    /// `8.5.6` → `80506`, as `php-config --vernum` prints it.
    fn php_vernum(&self) -> String {
        let mut parts = self.env.php_version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let (major, minor, patch) = (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        );
        (major * 10000 + minor * 100 + patch).to_string()
    }

    /// Runs a script: a sequence of statements.
    fn run(&mut self, script: &str, depth: usize) -> Result<(), String> {
        if depth > 64 {
            return Err("config.m4 nests too deeply to evaluate".to_string());
        }
        let mut cursor = Cursor::new(script);
        while let Some(statement) = cursor.next_statement() {
            self.statement(statement, depth)?;
        }
        Ok(())
    }

    /// Executes one parsed statement: dispatches a macro call, stores an expanded assignment, runs
    /// the first `if` or `elif` arm whose condition holds (or the `else` arm), records skipped
    /// shell constructs as notes, and ignores plain commands.
    fn statement(&mut self, statement: Statement<'_>, depth: usize) -> Result<(), String> {
        match statement {
            Statement::Macro { name, args } => self.call(name, &args, depth),
            Statement::Assign { name, value } => {
                let expanded = self.expand(value);
                self.vars.insert(name.to_string(), expanded);
                Ok(())
            }
            Statement::If { arms, otherwise } => {
                for (condition, body) in arms {
                    if self.condition(condition)? {
                        return self.run(body, depth + 1);
                    }
                }
                match otherwise {
                    Some(body) => self.run(body, depth + 1),
                    None => Ok(()),
                }
            }
            Statement::Skipped(what) => {
                self.note(format!("skipped shell construct: {}", what.trim()));
                Ok(())
            }
            Statement::Command(text) => self.command_list(text, depth),
        }
    }

    /// Runs a command line joined by `&&`/`||` as the shell does (`test -z "$X" || AC_MSG_ERROR(…)`)
    /// when every command in it is a `test`/`[ … ]` the evaluator understands, or a macro call or
    /// assignment it runs. A line holding any other command (a probe it cannot run) is skipped
    /// whole, with a note; a lone plain command is ignored as before.
    fn command_list(&mut self, text: &str, depth: usize) -> Result<(), String> {
        let parts = split_and_or(text);
        if parts.len() < 2 {
            return Ok(());
        }
        let mut classified = Vec::new();
        for (operator, part) in &parts {
            let trimmed = part.trim().trim_end_matches(';').trim();
            let bare = trimmed.strip_prefix('!').map_or(trimmed, str::trim_start);
            let test = bare.starts_with("test ") || (bare.starts_with('[') && bare.ends_with(']'));
            let runs = !test
                && matches!(
                    Cursor::new(trimmed).next_statement(),
                    Some(Statement::Macro { .. } | Statement::Assign { .. })
                );
            if !test && !runs {
                self.note(format!("skipped shell command: {}", text.trim()));
                return Ok(());
            }
            classified.push((*operator, test, trimmed));
        }
        let mut status = true;
        for (index, (operator, test, part)) in classified.into_iter().enumerate() {
            if index > 0 && (operator == "&&") != status {
                continue;
            }
            status = if test {
                self.command_condition(part)?
            } else {
                self.run(part, depth + 1)?;
                true
            };
        }
        Ok(())
    }

    /// Records a note about something the evaluator skipped or approximated, ignoring duplicates.
    fn note(&mut self, note: String) {
        if !self.config.notes.contains(&note) {
            self.config.notes.push(note);
        }
    }

    /// Dispatches one m4 macro. Arguments arrive with one level of `[ ]`
    /// quoting removed, as m4 would pass them.
    fn call(&mut self, name: &str, args: &[String], depth: usize) -> Result<(), String> {
        let arg = |index: usize| args.get(index).map(String::as_str).unwrap_or("");
        match name {
            "PHP_ARG_ENABLE" | "PHP_ARG_WITH" => {
                let option = arg(0).trim();
                let variable = format!("PHP_{}", option.to_ascii_uppercase().replace('-', "_"));
                let value = if option.eq_ignore_ascii_case(&self.env.extension) {
                    "yes".to_string()
                } else {
                    let default = arg(3).trim();
                    if default.is_empty() { "no".to_string() } else { default.to_string() }
                };
                self.vars.insert(variable, value);
            }
            "PHP_NEW_EXTENSION" => {
                self.config.extension = self.expand(arg(0)).trim().to_string();
                for source in self.expand(arg(1)).split_whitespace() {
                    self.add_source(source.to_string());
                }
                self.add_cflags(arg(4));
                if arg(5).trim() == "cxx" {
                    self.config.cxx = true;
                }
            }
            "PHP_ADD_SOURCES" | "PHP_ADD_SOURCES_X" => {
                let dir = self.expand(arg(0)).trim().to_string();
                for source in self.expand(arg(1)).split_whitespace() {
                    let joined = if dir.is_empty() { source.to_string() } else { format!("{dir}/{source}") };
                    self.add_source(joined);
                }
                self.add_cflags(arg(2));
            }
            "PHP_REQUIRE_CXX" => self.config.cxx = true,
            "PHP_CXX_COMPILE_STDCXX" => {
                self.config.cxx = true;
                let standard = arg(0).trim();
                if !standard.is_empty() {
                    self.config.cflags.push(format!("-std=c++{standard}"));
                }
            }
            "AC_DEFINE" | "AC_DEFINE_UNQUOTED" => {
                let define = self.expand(arg(0)).trim().to_string();
                if !define.is_empty() {
                    let value = if args.len() > 1 {
                        self.define_value(arg(1), name == "AC_DEFINE_UNQUOTED")
                    } else {
                        "1".to_string()
                    };
                    self.config.defines.insert(define, if value.is_empty() { "1".to_string() } else { value });
                }
            }
            "PHP_ADD_INCLUDE" => {
                let dir = self.expand(arg(0)).trim().to_string();
                if !dir.is_empty() && !self.config.include_dirs.contains(&dir) {
                    self.config.include_dirs.push(dir);
                }
            }
            "PHP_ADD_EXTENSION_DEP" => {
                let dependency = ExtensionDependency {
                    name: arg(1).trim().to_string(),
                    optional: arg(2).trim() == "true",
                };
                if !dependency.name.is_empty() && !self.config.dependencies.contains(&dependency) {
                    self.config.dependencies.push(dependency);
                }
            }
            "PHP_ADD_LIBRARY" | "PHP_ADD_LIBRARY_WITH_PATH" => {
                let library = self.expand(arg(0)).trim().to_string();
                self.add_library(library);
                if name == "PHP_ADD_LIBRARY_WITH_PATH" {
                    let dir = self.expand(arg(1)).trim().to_string();
                    self.add_library_dir(dir);
                }
            }
            // `PHP_EVAL_LIBLINE(-L/x -lfoo)` and `PHP_EVAL_INCLINE(-I/x -DY)`
            // split a flag line (usually pkg-config output) into link and
            // compile inputs; the flags themselves are modelled exactly.
            "PHP_EVAL_LIBLINE" => {
                let line = self.expand(arg(0));
                self.adopt_link_line(&line);
            }
            "PHP_EVAL_INCLINE" => {
                let line = self.expand(arg(0));
                self.adopt_compile_line(&line);
            }
            "PKG_CHECK_MODULES" => {
                // No pkg-config is run: its variables stay empty, and neither
                // branch is taken. Recorded so a failed build can say why.
                self.note(format!("PKG_CHECK_MODULES({}) was not evaluated", self.expand(arg(1)).trim()));
            }
            "AS_VAR_IF" => {
                let variable = arg(0).trim().trim_start_matches('$');
                let current = self.vars.get(variable).cloned().unwrap_or_default();
                let branch = if current == self.expand(arg(1)).trim() { arg(2) } else { arg(3) };
                self.run(branch, depth + 1)?;
            }
            "AS_IF" => {
                // AS_IF(test1, [run-if-true1], [test2, run-if-true2, ...], [run-if-false])
                let mut index = 0;
                loop {
                    if index + 1 >= args.len() {
                        if index < args.len() {
                            self.run(arg(index), depth + 1)?;
                        }
                        break;
                    }
                    if self.condition(arg(index))? {
                        self.run(arg(index + 1), depth + 1)?;
                        break;
                    }
                    index += 2;
                }
            }
            "AC_ARG_ENABLE" | "AC_ARG_WITH" => self.run(arg(3), depth + 1)?,
            "AC_CACHE_CHECK" => self.run(arg(2), depth + 1)?,
            "AC_CACHE_VAL" => self.run(arg(1), depth + 1)?,
            // Probes of the compiler and C library: every supported target is a
            // modern POSIX system, so they take their success branch.
            "AC_RUN_IFELSE" | "AC_COMPILE_IFELSE" | "AC_LINK_IFELSE" | "AC_PREPROC_IFELSE" => {
                self.run(arg(1), depth + 1)?
            }
            "AC_TRY_RUN" => self.run(arg(1), depth + 1)?,
            "AC_TRY_COMPILE" | "AC_TRY_LINK" => self.run(arg(2), depth + 1)?,
            "AX_CHECK_COMPILE_FLAG" => self.run(arg(1), depth + 1)?,
            // Each function is probed and takes its own action; only the plural form defines
            // `HAVE_*`, as in autoconf.
            "AC_CHECK_FUNCS" | "AC_CHECK_FUNC" => {
                let functions: Vec<String> = self.expand(arg(0)).split_whitespace().map(str::to_string).collect();
                for function in functions {
                    let found = self.function_links(&function);
                    if found && name == "AC_CHECK_FUNCS" {
                        self.config
                            .defines
                            .insert(format!("HAVE_{}", function.to_ascii_uppercase()), "1".to_string());
                    }
                    self.run(if found { arg(1) } else { arg(2) }, depth + 1)?;
                }
            }
            "AC_CHECK_HEADERS" | "AC_CHECK_HEADER" => {
                // Each header is compiled with the target toolchain and runs its own found or
                // not-found action, as autoconf expands the plural into one check per header.
                let headers: Vec<String> = self.expand(arg(0)).split_whitespace().map(str::to_string).collect();
                for header in headers {
                    let present = self.header_compiles(&header);
                    if present && name == "AC_CHECK_HEADERS" {
                        self.config.defines.insert(format!("HAVE_{}", shell_identifier(&header)), "1".to_string());
                    }
                    self.run(if present { arg(1) } else { arg(2) }, depth + 1)?;
                }
            }
            "AC_CHECK_LIB" | "PHP_CHECK_LIBRARY" => {
                // Autoconf's test: a program calling the function links with the library, the
                // other-libraries argument and what `LIBS` already holds.
                let library = self.expand(arg(0)).trim().to_string();
                let function = self.expand(arg(1)).trim().to_string();
                let function = if function.is_empty() { "main".to_string() } else { function };
                let extra = format!("-l{library} {}", self.expand(arg(4)));
                if !library.is_empty() && self.function_links_with(&function, &extra) {
                    // Autoconf's default success action links the library and defines
                    // HAVE_LIB<LIBRARY>; PHP's macro has none.
                    if name == "AC_CHECK_LIB" && arg(2).trim().is_empty() {
                        self.add_library(library.clone());
                        self.config.defines.insert(format!("HAVE_LIB{}", shell_identifier(&library)), "1".to_string());
                    }
                    self.run(arg(2), depth + 1)?;
                } else {
                    self.note(format!("library probe for '{library}' answered 'not found'"));
                    self.run(arg(3), depth + 1)?;
                }
            }
            "AC_C_BIGENDIAN" => self.run(arg(1), depth + 1)?,
            // Every supported target is LP64: the standard types have one size there.
            "AC_CHECK_SIZEOF" => {
                let ty = self.expand(arg(0)).trim().to_string();
                match lp64_sizeof(&ty) {
                    Some(size) => {
                        let define = format!("SIZEOF_{}", shell_identifier(&ty.replace('*', "p")));
                        self.config.defines.insert(define, size.to_string());
                    }
                    None => self.note(format!("size probe for '{ty}' not answered")),
                }
            }
            "ifdef" | "m4_ifdef" => {
                let defined = phpize_defines(arg(0).trim());
                self.run(if defined { arg(1) } else { arg(2) }, depth + 1)?;
            }
            "AC_MSG_ERROR" | "AC_MSG_FAILURE" => {
                return Err(format!("config.m4 stopped: {}", self.expand(arg(0)).trim()));
            }
            // Output, bookkeeping, and build-system plumbing phpize owns.
            "AC_MSG_CHECKING" | "AC_MSG_RESULT" | "AC_MSG_NOTICE" | "AC_MSG_WARN"
            | "PHP_SUBST" | "PHP_ADD_MAKEFILE_FRAGMENT" | "PHP_INSTALL_HEADERS" | "PHP_ADD_BUILD_DIR"
            | "AC_LANG_PUSH" | "AC_LANG_POP" | "AC_LANG_PROGRAM" | "AC_LANG_SOURCE" | "AC_REQUIRE"
            | "AC_PROG_CC" | "AC_PROG_CXX" | "AC_CONFIG_HEADERS" | "AC_CHECK_TYPES"
            | "AC_CHECK_DECLS" | "AC_CHECK_DECL" | "AC_PATH_PROG"
            | "AC_SUBST" | "AS_HELP_STRING"
            | "PHP_ALWAYS_SHARED" | "AC_CONFIG_FILES" | "AC_OUTPUT" => {}
            other => self.note(format!("ignored macro {other}")),
        }
        Ok(())
    }

    /// Adds a source file to the extension's list, made relative to the extension directory and
    /// without a leading `./`; empty and duplicate entries are ignored.
    fn add_source(&mut self, source: String) {
        let ext_dir = self.env.extension_dir.display().to_string();
        let relative = source
            .strip_prefix(&format!("{ext_dir}/"))
            .map(str::to_string)
            .unwrap_or(source);
        let relative = relative.trim_start_matches("./").to_string();
        if !relative.is_empty() && !self.config.sources.contains(&relative) {
            self.config.sources.push(relative);
        }
    }

    /// Expands variables in a flag string and appends each whitespace-separated flag, with
    /// surrounding double quotes removed, to the extension's compiler flags.
    fn add_cflags(&mut self, flags: &str) {
        for flag in self.expand(flags).split_whitespace() {
            self.config.cflags.push(flag.trim_matches('"').to_string());
        }
    }

    /// Adds a library to link, ignoring empty and duplicate names.
    fn add_library(&mut self, library: String) {
        if !library.is_empty() && !self.config.libraries.contains(&library) {
            self.config.libraries.push(library);
        }
    }

    /// Adds a library search directory, ignoring empty and duplicate entries.
    fn add_library_dir(&mut self, dir: String) {
        if !dir.is_empty() && !self.config.library_dirs.contains(&dir) {
            self.config.library_dirs.push(dir);
        }
    }

    /// Splits an expanded link line into `-l` libraries and `-L` search directories; any other
    /// flag is noted rather than guessed at.
    fn adopt_link_line(&mut self, line: &str) {
        for flag in line.split_whitespace() {
            if let Some(library) = flag.strip_prefix("-l") {
                self.add_library(library.to_string());
            } else if let Some(dir) = flag.strip_prefix("-L") {
                self.add_library_dir(dir.to_string());
            } else {
                self.note(format!("ignored link flag {flag}"));
            }
        }
    }

    /// Splits an expanded compile line into `-I` include directories and compiler flags.
    fn adopt_compile_line(&mut self, line: &str) {
        for flag in line.split_whitespace() {
            if let Some(dir) = flag.strip_prefix("-I") {
                if !dir.is_empty() && !self.config.include_dirs.iter().any(|known| known == dir) {
                    self.config.include_dirs.push(dir.to_string());
                }
            } else if !self.config.cflags.iter().any(|known| known == flag) {
                self.config.cflags.push(flag.to_string());
            }
        }
    }

    /// Adopts the final values of the global flag variables configure would
    /// have exported to the build: `CPPFLAGS` and `CFLAGS` reach every unit,
    /// `CXXFLAGS` the C++ ones, `LDFLAGS` and `LIBS` the link. A script that
    /// saves and restores them around a probe leaves nothing behind, as
    /// configure would.
    fn adopt_global_flags(&mut self) {
        for name in ["CPPFLAGS", "CFLAGS"] {
            let line = self.vars.get(name).cloned().unwrap_or_default();
            self.adopt_compile_line(&line);
        }
        let cxx_line = self.vars.get("CXXFLAGS").cloned().unwrap_or_default();
        for flag in cxx_line.split_whitespace() {
            if !self.config.cxxflags.iter().any(|known| known == flag) {
                self.config.cxxflags.push(flag.to_string());
            }
        }
        for name in ["LDFLAGS", "LIBS"] {
            let line = self.vars.get(name).cloned().unwrap_or_default();
            self.adopt_link_line(&line);
        }
    }

    /// A define's replacement text as configure writes it into confdefs.h,
    /// through a heredoc: C quotes survive (`["1.2.3"]` stays a string
    /// literal), and only `AC_DEFINE_UNQUOTED` expands `$var` and backticks.
    fn define_value(&self, text: &str, unquoted: bool) -> String {
        if !unquoted {
            return text.trim().to_string();
        }
        // A heredoc keeps quotes that the shell-word expansion would strip:
        // shield them, expand, then put them back.
        const ESCAPED_QUOTE: char = '\u{1}';
        const QUOTE: char = '\u{2}';
        const APOSTROPHE: char = '\u{3}';
        let shielded = text.replace("\\\"", &ESCAPED_QUOTE.to_string()).replace('"', &QUOTE.to_string()).replace('\'', &APOSTROPHE.to_string());
        self.expand_in(&shielded, true)
            .replace(ESCAPED_QUOTE, "\\\"")
            .replace(QUOTE, "\"")
            .replace(APOSTROPHE, "'")
            .trim()
            .to_string()
    }

    /// Expands shell text: variables, `${VAR}`, backtick commands and directory macros, handling
    /// backslash escapes and quotes, and drops the quoting characters themselves.
    fn expand(&self, text: &str) -> String {
        self.expand_in(text, false)
    }

    /// [`Self::expand`], with `heredoc` reading the whole text the way configure's unquoted
    /// heredoc does (as inside double quotes). There, as between double quotes, a backslash
    /// escapes only `\`, `$`, `` ` ``, `"` and a newline; before any other character it stays
    /// (`"a\db"` is `a\db`, `["l1\nl2"]` keeps its `\n`), and an apostrophe is a plain character.
    fn expand_in(&self, text: &str, heredoc: bool) -> String {
        let text = &self.expand_dir_macros(text);
        let bytes = text.as_bytes();
        let mut out = String::new();
        let mut in_double = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                // `\$(VAR)` defers a variable to make; the value is the same.
                b'\\' if bytes.get(i + 1) == Some(&b'$') => i += 1,
                b'\\' if i + 1 < bytes.len() => {
                    let escaped = text[i + 1..].chars().next().unwrap_or(' ');
                    if (in_double || heredoc) && !matches!(escaped, '\\' | '`' | '"' | '\n') {
                        out.push('\\');
                    }
                    out.push(if escaped == '\n' { ' ' } else { escaped });
                    i += 1 + escaped.len_utf8();
                }
                b'"' => {
                    in_double = !in_double;
                    i += 1;
                }
                b'\'' if in_double || heredoc => {
                    out.push('\'');
                    i += 1;
                }
                b'\'' => {
                    let end = text[i + 1..].find('\'').map(|p| i + 1 + p).unwrap_or(bytes.len());
                    out.push_str(&text[i + 1..end]);
                    i = end + 1;
                }
                b'`' => {
                    let end = text[i + 1..].find('`').map(|p| i + 1 + p).unwrap_or(bytes.len());
                    out.push_str(&self.command_output(&text[i + 1..end]));
                    i = end + 1;
                }
                b'$' => {
                    let rest = &text[i + 1..];
                    if let Some(inner) = rest.strip_prefix('(') {
                        // A make-style reference, `$(VAR)`.
                        let end = inner.find(')').unwrap_or(inner.len());
                        out.push_str(self.vars.get(inner[..end].trim()).map(String::as_str).unwrap_or(""));
                        i += 2 + end + 1;
                    } else if let Some(inner) = rest.strip_prefix('{') {
                        let end = matching_brace(inner);
                        out.push_str(&self.parameter(&inner[..end]));
                        i += 2 + end + 1;
                    } else {
                        let len = rest.bytes().take_while(|b| is_word_byte(*b)).count();
                        if len == 0 {
                            out.push('$');
                        } else {
                            out.push_str(self.vars.get(&rest[..len]).map(String::as_str).unwrap_or(""));
                        }
                        i += 1 + len;
                    }
                }
                other => {
                    let ch = text[i..].chars().next().unwrap_or(other as char);
                    out.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        out
    }

    /// Expands the body of a `${…}` parameter reference: a plain `NAME`, or
    /// one with a `-`, `=`, `+` or `?` operator, optionally prefixed by `:` to
    /// treat an empty value as unset. `=` cannot assign during expansion, so
    /// it behaves like `-`; `?` yields the value or nothing.
    fn parameter(&self, body: &str) -> String {
        let name_len = body.bytes().take_while(|b| is_word_byte(*b)).count();
        let (name, operation) = body.split_at(name_len);
        let value = self.vars.get(name);
        let (colon, operation) = match operation.strip_prefix(':') {
            Some(rest) => (true, rest),
            None => (false, operation),
        };
        let set = match value {
            Some(value) => !colon || !value.is_empty(),
            None => false,
        };
        let current = || value.cloned().unwrap_or_default();
        let mut chars = operation.chars();
        match chars.next() {
            Some('-' | '=') if !set => self.expand(chars.as_str()),
            Some('+') if set => self.expand(chars.as_str()),
            Some('+') => String::new(),
            _ => current(),
        }
    }

    /// Replaces `PHP_EXT_SRCDIR(x)`, `PHP_EXT_BUILDDIR(x)` and `PHP_EXT_DIR(x)`
    /// — macros m4 expands inside other macros' arguments — with the extension
    /// directory.
    fn expand_dir_macros(&self, text: &str) -> String {
        let mut out = text.to_string();
        let dir = self.env.extension_dir.display().to_string();
        for name in ["PHP_EXT_SRCDIR(", "PHP_EXT_BUILDDIR(", "PHP_EXT_DIR("] {
            while let Some(start) = out.find(name) {
                let Some(close) = out[start..].find(')') else { break };
                out.replace_range(start..start + close + 1, &dir);
            }
        }
        out
    }

    /// The output of a backtick command. Only `php-config` queries are
    /// answered; anything else produces nothing, as a failing command would.
    fn command_output(&self, command: &str) -> String {
        let command = self.expand(command);
        let command = command.trim();
        if let Some(query) = command.strip_prefix("php-config") {
            return match query.trim() {
                "--vernum" => self.php_vernum(),
                "--version" => self.env.php_version.clone(),
                "--include-dir" => self.env.php_include_dir.display().to_string(),
                _ => String::new(),
            };
        }
        String::new()
    }

    /// Evaluates a shell condition: commands joined by `&&` and `||` (equal precedence, left to
    /// right, as the shell runs them), each a `test …` or `[ … ]`, negated by a leading `!`, or
    /// anything else (false). A `test` whose words are not all understood is an error rather
    /// than a guess at the branch.
    fn condition(&self, text: &str) -> Result<bool, String> {
        let mut commands = split_and_or(text).into_iter();
        let Some((_, first)) = commands.next() else { return Ok(false) };
        let mut result = self.command_condition(first)?;
        for (operator, command) in commands {
            let value = self.command_condition(command)?;
            result = if operator == "&&" { result && value } else { result || value };
        }
        Ok(result)
    }

    /// Evaluates one command of a condition: `test …`, `[ … ]`, negated by a leading `!`
    /// pipeline operator, or anything else (false).
    fn command_condition(&self, text: &str) -> Result<bool, String> {
        let text = text.trim().trim_end_matches(';').trim();
        if let Some(rest) = text.strip_prefix('!').filter(|rest| rest.starts_with([' ', '\t'])) {
            return Ok(!self.command_condition(rest)?);
        }
        let body = if let Some(rest) = text.strip_prefix("test ") {
            rest
        } else if let Some(rest) = text.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            rest
        } else {
            return Ok(false);
        };
        let words = self.words(body);
        let mut position = 0;
        let result = self.test_or(&words, &mut position);
        if position != words.len() {
            return Err(format!("config.m4 condition not understood: {text}"));
        }
        Ok(result)
    }

    /// Splits `test` arguments into words, honouring quotes, then expands them.
    fn words(&self, text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let mut in_word = false;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' | '\'' | '`' => {
                    in_word = true;
                    current.push(c);
                    for inner in chars.by_ref() {
                        current.push(inner);
                        if inner == c {
                            break;
                        }
                    }
                }
                c if c.is_whitespace() => {
                    if in_word {
                        words.push(self.expand(&current));
                        current.clear();
                        in_word = false;
                    }
                }
                c => {
                    in_word = true;
                    current.push(c);
                }
            }
        }
        if in_word {
            words.push(self.expand(&current));
        }
        words
    }

    /// Evaluates `-o` (or) chains in a `test` expression, each operand being an `-a` chain,
    /// advancing `position` through the words.
    fn test_or(&self, words: &[String], position: &mut usize) -> bool {
        let mut result = self.test_and(words, position);
        while words.get(*position).map(String::as_str) == Some("-o") {
            *position += 1;
            let right = self.test_and(words, position);
            result = result || right;
        }
        result
    }

    /// Evaluates `-a` (and) chains in a `test` expression, advancing `position` through the words.
    fn test_and(&self, words: &[String], position: &mut usize) -> bool {
        let mut result = self.test_term(words, position);
        while words.get(*position).map(String::as_str) == Some("-a") {
            *position += 1;
            let right = self.test_term(words, position);
            result = result && right;
        }
        result
    }

    /// Evaluates one `test` term: negation with `!`, unary string and file tests (`-z -n -f -d -e
    /// -x -r -s`, relative paths resolved against the extension directory), binary string and
    /// integer comparisons, or the truthiness of a lone word.
    fn test_term(&self, words: &[String], position: &mut usize) -> bool {
        let Some(first) = words.get(*position) else { return false };
        if first == "!" {
            *position += 1;
            return !self.test_term(words, position);
        }
        const UNARY: &[&str] = &["-z", "-n", "-f", "-d", "-e", "-x", "-r", "-s"];
        if UNARY.contains(&first.as_str()) && *position + 1 < words.len() {
            let operand = &words[*position + 1];
            *position += 2;
            return match first.as_str() {
                "-z" => operand.is_empty(),
                "-n" => !operand.is_empty(),
                _ => {
                    let path = Path::new(operand);
                    let path = if path.is_absolute() { path.to_path_buf() } else { self.env.extension_dir.join(path) };
                    file_test(first, &path)
                }
            };
        }
        if let Some(op) = words.get(*position + 1) {
            let binary = ["=", "==", "!=", "-eq", "-ne", "-lt", "-le", "-gt", "-ge"];
            if binary.contains(&op.as_str()) {
                let left = first.clone();
                let right = words.get(*position + 2).cloned().unwrap_or_default();
                *position += 3;
                let numeric = |s: &str| s.trim().parse::<i64>().ok();
                return match op.as_str() {
                    "=" | "==" => left == right,
                    "!=" => left != right,
                    _ => match (numeric(&left), numeric(&right)) {
                        (Some(l), Some(r)) => match op.as_str() {
                            "-eq" => l == r,
                            "-ne" => l != r,
                            "-lt" => l < r,
                            "-le" => l <= r,
                            "-gt" => l > r,
                            _ => l >= r,
                        },
                        _ => false,
                    },
                };
            }
        }
        *position += 1;
        !first.is_empty()
    }
}

/// Splits a shell condition at the `&&` and `||` outside quotes: each command with the operator
/// joining it to the previous one (empty for the first).
fn split_and_or(text: &str) -> Vec<(&'static str, &str)> {
    let bytes = text.as_bytes();
    let mut commands = Vec::new();
    let mut operator = "";
    let mut start = 0;
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if matches!(byte, b'"' | b'\'' | b'`') => quote = Some(byte),
            None if index + 1 < bytes.len() && matches!(&bytes[index..index + 2], b"&&" | b"||") => {
                commands.push((operator, &text[start..index]));
                operator = if byte == b'&' { "&&" } else { "||" };
                index += 2;
                start = index;
                continue;
            }
            None => {}
        }
        index += 1;
    }
    commands.push((operator, &text[start..]));
    commands
}

/// Answers a `test` file operator for `path` as the shell would: `-f` a
/// regular file, `-d` a directory, `-x` searchable or executable by someone,
/// `-r` openable for reading, `-s` non-empty, `-e` anything that exists.
fn file_test(operator: &str, path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(path) else { return false };
    match operator {
        "-f" => metadata.is_file(),
        "-d" => metadata.is_dir(),
        "-x" => metadata.permissions().mode() & 0o111 != 0,
        "-r" => {
            if metadata.is_dir() {
                std::fs::read_dir(path).is_ok()
            } else {
                std::fs::File::open(path).is_ok()
            }
        }
        "-s" => metadata.len() > 0,
        _ => true,
    }
}

/// The offset of the `}` closing a `${` reference whose body starts `inner`,
/// counting nested braces; the text length if it is unterminated.
fn matching_brace(inner: &str) -> usize {
    let mut depth = 0usize;
    for (offset, byte) in inner.bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' if depth == 0 => return offset,
            b'}' => depth -= 1,
            _ => {}
        }
    }
    inner.len()
}

/// One statement of the script, borrowing from its text.
enum Statement<'s> {
    Macro { name: &'s str, args: Vec<String> },
    Assign { name: &'s str, value: &'s str },
    If { arms: Vec<(&'s str, &'s str)>, otherwise: Option<&'s str> },
    Skipped(&'s str),
    /// A plain command line, possibly a `&&`/`||` list.
    Command(&'s str),
}

/// Reads statements from shell + m4 text.
struct Cursor<'s> {
    text: &'s str,
    position: usize,
}

impl<'s> Cursor<'s> {
    /// Creates a cursor at the start of the script text.
    fn new(text: &'s str) -> Self {
        Self { text, position: 0 }
    }

    /// The script text as bytes.
    fn bytes(&self) -> &'s [u8] {
        self.text.as_bytes()
    }

    /// Skips whitespace, line continuations, statement separators and `#`
    /// comments.
    fn skip_blank(&mut self) {
        let bytes = self.bytes();
        while self.position < bytes.len() {
            match bytes[self.position] {
                b' ' | b'\t' | b'\n' | b'\r' | b';' => self.position += 1,
                b'\\' if bytes.get(self.position + 1) == Some(&b'\n') => self.position += 2,
                b'#' => {
                    while self.position < bytes.len() && bytes[self.position] != b'\n' {
                        self.position += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// The identifier-like word starting at byte offset `at`, possibly empty.
    fn word_at(&self, at: usize) -> &'s str {
        let bytes = self.bytes();
        let len = bytes[at..].iter().take_while(|b| is_word_byte(**b)).count();
        &self.text[at..at + len]
    }

    /// Reads the next statement at the cursor: an `if`, a skipped `for`/`while`/`until`/`case`
    /// construct, a macro call with its arguments, a variable assignment, or a plain command; None
    /// at the end of the text.
    fn next_statement(&mut self) -> Option<Statement<'s>> {
        self.skip_blank();
        let bytes = self.bytes();
        if self.position >= bytes.len() {
            return None;
        }
        let start = self.position;
        let word = self.word_at(start);
        let after = start + word.len();
        match word {
            "if" => return Some(self.read_if()),
            "for" | "while" | "until" => {
                let end = self.find_closing(after, word, "done");
                self.position = end;
                return Some(Statement::Skipped(&self.text[start..end]));
            }
            "case" => {
                let end = self.find_closing(after, "case", "esac");
                self.position = end;
                return Some(Statement::Skipped(&self.text[start..end]));
            }
            _ => {}
        }
        // A macro call or assignment followed by `&&`/`||` heads a command list
        // (`X=yes && Y=yes`): the whole line is read as one command so the list runs as a unit.
        if !word.is_empty() && bytes.get(after) == Some(&b'(') {
            let (args, end) = read_macro_args(self.text, after + 1);
            if !self.continues_list(end) {
                self.position = end;
                return Some(Statement::Macro { name: word, args });
            }
        } else if !word.is_empty() && bytes.get(after) == Some(&b'=') {
            let value_end = self.shell_word_end(after + 1);
            if !self.continues_list(value_end) {
                self.position = value_end;
                return Some(Statement::Assign { name: word, value: &self.text[after + 1..value_end] });
            }
        }
        let end = self.command_end(start);
        self.position = end.max(start + 1);
        Some(Statement::Command(&self.text[start..end.max(start)]))
    }

    /// Whether `&&` or `||` follows offset `at` on the same line, past blanks.
    fn continues_list(&self, at: usize) -> bool {
        let rest = self.bytes().get(at..).unwrap_or_default();
        let blanks = rest.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
        matches!(rest.get(blanks..blanks + 2), Some(b"&&" | b"||"))
    }

    /// End of one shell word: stops at unquoted whitespace or `;`.
    fn shell_word_end(&self, from: usize) -> usize {
        let bytes = self.bytes();
        let mut i = from;
        while i < bytes.len() {
            match bytes[i] {
                b'"' | b'\'' | b'`' => i = skip_quoted(bytes, i),
                b'\\' => i += 2,
                b' ' | b'\t' | b'\n' | b';' => break,
                _ => i += 1,
            }
        }
        i.min(bytes.len())
    }

    /// End of a simple command: the end of its line or a `;`, honouring
    /// quotes, brackets and parentheses so a multi-line macro argument inside
    /// the command is not split.
    fn command_end(&self, from: usize) -> usize {
        let bytes = self.bytes();
        let mut i = from;
        let mut depth = 0i32;
        while i < bytes.len() {
            match bytes[i] {
                b'"' | b'\'' | b'`' => {
                    i = skip_quoted(bytes, i);
                    continue;
                }
                b'\\' => {
                    i += 2;
                    continue;
                }
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b'\n' | b';' if depth <= 0 => break,
                _ => {}
            }
            i += 1;
        }
        i.min(bytes.len())
    }

    /// Finds the position just past the keyword closing a block opened by
    /// `open` at `from`, counting nested openers.
    fn find_closing(&self, from: usize, open: &str, close: &str) -> usize {
        let mut depth = 1;
        let mut i = from;
        let bytes = self.bytes();
        while i < bytes.len() {
            if let Some(next) = self.keyword_at(i) {
                if next == close {
                    depth -= 1;
                    if depth == 0 {
                        return i + close.len();
                    }
                } else if next == open || (open != "case" && matches!(next, "for" | "while" | "until")) {
                    depth += 1;
                }
                i += next.len();
                continue;
            }
            i = self.advance(i);
        }
        bytes.len()
    }

    /// Moves past one character, or a whole quoted/bracketed span, so every position the scans
    /// stop at is a character boundary of the text (a comment may hold non-ASCII text).
    fn advance(&self, i: usize) -> usize {
        let bytes = self.bytes();
        match bytes[i] {
            b'"' | b'\'' | b'`' => skip_quoted(bytes, i),
            b'[' => skip_bracketed(bytes, i),
            b'\\' => self.char_end(i + 1),
            _ => self.char_end(i),
        }
    }

    /// The offset just past the character starting at `i`, at most the end of the text.
    fn char_end(&self, i: usize) -> usize {
        let width = self.text.get(i..).and_then(|rest| rest.chars().next()).map_or(1, char::len_utf8);
        (i + width).min(self.text.len())
    }

    /// The keyword starting at `i`, if `i` begins a word in command position.
    fn keyword_at(&self, i: usize) -> Option<&'s str> {
        let bytes = self.bytes();
        if !self.text.is_char_boundary(i) || (i > 0 && is_word_byte(bytes[i - 1])) {
            return None;
        }
        let word = self.word_at(i);
        let end = i + word.len();
        let delimited = bytes.get(end).is_none_or(|b| matches!(b, b' ' | b'\t' | b'\n' | b';' | b'\r'));
        let command_position = i == 0
            || self.text[..i]
                .trim_end_matches([' ', '\t'])
                .ends_with(['\n', ';', '(', ')'])
            || self.text[..i].trim_end_matches([' ', '\t']).is_empty()
            || matches!(self.previous_word(i), Some("then" | "else" | "do"));
        (delimited && command_position && !word.is_empty()).then_some(word)
    }

    /// The word immediately before byte offset `i` once trailing whitespace is skipped, or None
    /// when there is none.
    fn previous_word(&self, i: usize) -> Option<&'s str> {
        let before = self.text[..i].trim_end();
        let start = before
            .char_indices()
            .rfind(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(0, |(p, c)| p + c.len_utf8());
        let word = &before[start..];
        (!word.is_empty()).then_some(word)
    }

    /// Reads `if cond; then … [elif cond; then …] [else …] fi`.
    fn read_if(&mut self) -> Statement<'s> {
        let bytes = self.bytes();
        let mut arms = Vec::new();
        let mut otherwise = None;
        let mut i = self.position + 2;
        let mut condition_start = i;
        let mut body_start = None;
        let mut in_else = false;
        let mut depth = 0;
        while i < bytes.len() {
            if let Some(keyword) = self.keyword_at(i) {
                match keyword {
                    "if" => depth += 1,
                    "fi" if depth > 0 => depth -= 1,
                    "then" if depth == 0 && body_start.is_none() => {
                        body_start = Some(i + 4);
                    }
                    "elif" | "else" | "fi" if depth == 0 => {
                        let body = &self.text[body_start.unwrap_or(i)..i];
                        if in_else {
                            otherwise = Some(body);
                        } else {
                            let condition = &self.text[condition_start..body_start.map(|b| b - 4).unwrap_or(i)];
                            arms.push((condition, body));
                        }
                        match keyword {
                            "fi" => {
                                self.position = i + 2;
                                return Statement::If { arms, otherwise };
                            }
                            "elif" => {
                                condition_start = i + 4;
                                body_start = None;
                            }
                            _ => {
                                in_else = true;
                                body_start = Some(i + 4);
                            }
                        }
                    }
                    _ => {}
                }
                i += keyword.len();
                continue;
            }
            i = self.advance(i);
        }
        self.position = bytes.len();
        Statement::Skipped(&self.text[condition_start.saturating_sub(2)..])
    }
}

/// Returns the offset just past the quoted section that starts at `start`, honouring backslash
/// escapes except inside single quotes; the text length if it is unterminated.
fn skip_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && quote != b'\'' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// Returns the offset just past the `[ ... ]` quoted group that starts at `start`, counting nested
/// brackets; the text length if it is unterminated.
fn skip_bracketed(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Reads m4 macro arguments starting just after `(`. Returns the arguments with
/// one level of `[ ]` quoting removed and leading whitespace dropped, as m4
/// passes them, and the position just past the closing `)`.
fn read_macro_args(text: &str, from: usize) -> (Vec<String>, usize) {
    let bytes = text.as_bytes();
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = 0usize;
    let mut parens = 0usize;
    let mut i = from;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'[' => {
                if quote > 0 {
                    current.push('[');
                }
                quote += 1;
            }
            b']' if quote > 0 => {
                quote -= 1;
                if quote > 0 {
                    current.push(']');
                }
            }
            b'(' if quote == 0 => {
                parens += 1;
                current.push('(');
            }
            b')' if quote == 0 => {
                if parens == 0 {
                    args.push(finish_arg(&current));
                    return (args, i + 1);
                }
                parens -= 1;
                current.push(')');
            }
            b',' if quote == 0 && parens == 0 => {
                args.push(finish_arg(&current));
                current.clear();
            }
            _ => {
                let ch = text[i..].chars().next().unwrap_or(c as char);
                current.push(ch);
                i += ch.len_utf8();
                continue;
            }
        }
        i += 1;
    }
    args.push(finish_arg(&current));
    (args, bytes.len())
}

/// Cleans one macro argument: trims leading whitespace and trailing whitespace and
/// line-continuation backslashes.
fn finish_arg(raw: &str) -> String {
    raw.trim_start().trim_end_matches([' ', '\t', '\n', '\r', '\\']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: an `Environment` for the named extension with placeholder directories and PHP
    /// version 8.5.6.
    fn env(extension: &str) -> Environment {
        Environment {
            extension: extension.to_string(),
            extension_dir: PathBuf::from("/nonexistent/ext"),
            php_include_dir: PathBuf::from("/nonexistent/include"),
            php_version: "8.5.6".to_string(),
            // `missing_*` functions are absent, and a link naming a library the C runtime does not
            // ship (`yaml`, `rt` on macOS) fails; every other one links.
            function_probe: FunctionProbe::new(|function, libraries, _| {
                !function.starts_with("missing_")
                    && libraries.iter().all(|library| ["pthread", "m", "dl", "c"].contains(&library.as_str()))
            }),
            // `no_such_*` and `missing_*` headers are absent; every other one compiles.
            header_probe: HeaderProbe::new(|header, _| {
                !header.starts_with("no_such") && !header.starts_with("missing_")
            }),
        }
    }

    /// Non-ASCII text, in a shell comment or inside quotes, survives `dnl`
    /// stripping byte for byte.
    #[test]
    fn dnl_stripping_keeps_utf8_text() {
        let text = "# café ☕\ndnl gone é\nAC_MSG_CHECKING([naïve]) dnl gone too\n";
        assert_eq!(strip_dnl(text), "# café ☕\n\nAC_MSG_CHECKING([naïve]) \n");
    }

    /// simdjson's real config.m4, trimmed of comments: a version probe through
    /// `php-config` that must not reach its AC_MSG_ERROR, C++ sources, and
    /// flags passed as PHP_NEW_EXTENSION's fifth argument.
    const SIMDJSON: &str = r#"
PHP_ARG_ENABLE(simdjson, whether to enable simdjson, [ --enable-simdjson   Enable simdjson])

if test "$PHP_SIMDJSON" != "no"; then
  PHP_REQUIRE_CXX()
  AC_MSG_CHECKING([PHP version])
  if test -z "$PHP_CONFIG"; then
    AC_MSG_ERROR([php-config not found])
  fi
  php_version=`$PHP_CONFIG --vernum`
  if test -z "$php_version"; then
    AC_MSG_ERROR([failed to detect PHP version, please report])
  fi
  if test "$php_version" -lt "70000"; then
    AC_MSG_ERROR([You need at least PHP 7.0.0])
  else
    AC_MSG_RESULT([$php_version, ok])
  fi
  AX_CHECK_COMPILE_FLAG([-fvisibility=hidden],
                        [CXXFLAGS="$CXXFLAGS -fvisibility=hidden"])
  AC_DEFINE(HAVE_SIMDJSON, 1, [whether simdjson is enabled])
  dnl Disable exceptions because PHP is written in C
  PHP_NEW_EXTENSION(simdjson, [
      php_simdjson.cpp                    \
      src/simdjson_bindings.cpp           \
      src/simdjson.cpp],
    $ext_shared,, "-std=c++17 -DZEND_ENABLE_STATIC_TSRMLS_CACHE=1 -DSIMDJSON_EXCEPTIONS=0", cxx)
  PHP_INSTALL_HEADERS([ext/simdjson], [php_simdjson.h src/simdjson_bindings_defs.h])
  PHP_ADD_MAKEFILE_FRAGMENT
  PHP_ADD_BUILD_DIR(src, 1)
fi
"#;

    /// Verifies a real simdjson `config.m4` evaluates to its C++ sources, the C++ flag and compile
    /// definitions, the `HAVE_SIMDJSON` define and the extension name.
    #[test]
    fn evaluates_simdjson() {
        let config = evaluate(SIMDJSON, &env("simdjson")).expect("evaluates");
        assert_eq!(config.extension, "simdjson");
        assert_eq!(
            config.sources,
            vec!["php_simdjson.cpp", "src/simdjson_bindings.cpp", "src/simdjson.cpp"]
        );
        assert!(config.cxx);
        assert!(config.cflags.contains(&"-std=c++17".to_string()));
        assert!(config.cflags.contains(&"-DSIMDJSON_EXCEPTIONS=0".to_string()));
        assert_eq!(config.defines.get("HAVE_SIMDJSON").map(String::as_str), Some("1"));
    }

    /// APCu's shape: option defaults decide AS_VAR_IF arms, sources come from a
    /// variable, and four mutually exclusive lock backends sit behind probes.
    /// Only the one a POSIX system would pick may be defined.
    const APCU: &str = r#"
PHP_ARG_ENABLE([apcu],
  [whether to enable APCu support],
  [AS_HELP_STRING([--enable-apcu],
    [Enable APCu support])])

PHP_ARG_ENABLE([apcu-rwlocks],
  [if APCu should be allowed to use rwlocks],
  [AS_HELP_STRING([--disable-apcu-rwlocks], [Disable rwlocks in APCu])],
  [yes],
  [no])

PHP_ARG_ENABLE([apcu-debug],
  [if APCu should be built in debug mode],
  [AS_HELP_STRING([--enable-apcu-debug], [Enable APCu debugging])],
  [no],
  [no])

PHP_ARG_ENABLE([apcu-mmap],
  [if APCu should use mmap instead of shm],
  [AS_HELP_STRING([--disable-apcu-mmap], [Disable mmap, falls back on shm])],
  [yes],
  [no])

if test "$PHP_APCU" != "no"; then
  AS_VAR_IF([PHP_APCU_DEBUG], [no], [],
    [AC_DEFINE([APC_DEBUG], [1],
      [Define to 1 if APCu debugging mode is enabled.])])

  AS_VAR_IF([PHP_APCU_MMAP], [no], [],
    [AC_DEFINE([APC_MMAP], [1],
      [Define to 1 if APCu uses mmap instead of shm.])])

  if test "$PHP_APCU_RWLOCKS" != "no"; then
      orig_LIBS="$LIBS"
      LIBS="$LIBS -lpthread"
      AC_RUN_IFELSE([AC_LANG_SOURCE([[
#include <pthread.h>
main() { return 0; }
          ]])],[ dnl -Success-
          APCU_CFLAGS="-D_GNU_SOURCE -DZEND_ENABLE_STATIC_TSRMLS_CACHE=1"
          PHP_ADD_LIBRARY(pthread)
          AC_DEFINE(APC_NATIVE_RWLOCK, 1, [ ])
          AC_MSG_WARN([APCu has access to native rwlocks])
      ],[ dnl -Failure-
          AC_MSG_WARN([It doesn't appear that pthread rwlocks are supported])
          PHP_APCU_RWLOCKS=no
      ],[
          APCU_CFLAGS="-D_GNU_SOURCE -DZEND_ENABLE_STATIC_TSRMLS_CACHE=1"
      ])
    LIBS="$orig_LIBS"
  fi

  if test "$PHP_APCU_RWLOCKS" = "no"; then
   if test "$PHP_APCU_MUTEX" = "no"; then
      AS_VAR_IF([PHP_APCU_SPINLOCKS], [no], [
        AC_DEFINE([APC_FCNTL_LOCK], [1], [Define to 1 if APCu file locking is enabled.])
      ], [
        AC_DEFINE([APC_SPIN_LOCK], [1], [Define to 1 if APCu spin locking is enabled.])
      ])
   fi
  fi

  AC_CHECK_FUNCS(sigaction)

  for i in -Wall -Wextra -Wno-clobbered -Wno-unused-parameter; do
    AX_CHECK_COMPILE_FLAG([$i], [APCU_CFLAGS="$APCU_CFLAGS $i"])
  done

  apc_sources="apc.c apc_lock.c php_apc.c \
                 apc_cache.c \
                 apc_sma.c"

  PHP_CHECK_LIBRARY(rt, shm_open, [PHP_ADD_LIBRARY(rt,,APCU_SHARED_LIBADD)])
  PHP_NEW_EXTENSION(apcu, $apc_sources, $ext_shared,, \$(APCU_CFLAGS))
  PHP_SUBST(APCU_SHARED_LIBADD)
  PHP_INSTALL_HEADERS(ext/apcu, [php_apc.h apc.h])
  AC_DEFINE(HAVE_APCU, 1, [ ])
fi
"#;

    /// Verifies a real APCu `config.m4` evaluates to its sources and defaults (mmap on, debug off),
    /// defines exactly one lock backend, and links pthread.
    #[test]
    fn evaluates_apcu_choosing_exactly_one_lock_backend() {
        let config = evaluate(APCU, &env("apcu")).expect("evaluates");
        assert_eq!(config.extension, "apcu");
        assert_eq!(config.sources, vec!["apc.c", "apc_lock.c", "php_apc.c", "apc_cache.c", "apc_sma.c"]);
        assert!(config.defines.contains_key("APC_NATIVE_RWLOCK"), "{:?}", config.defines);
        assert!(config.defines.contains_key("APC_MMAP"), "mmap defaults to yes");
        assert!(!config.defines.contains_key("APC_DEBUG"), "debug defaults to no");
        assert!(!config.defines.contains_key("APC_FCNTL_LOCK"), "only one lock backend");
        assert!(!config.defines.contains_key("APC_SPIN_LOCK"), "only one lock backend");
        assert!(config.defines.contains_key("HAVE_SIGACTION"));
        assert_eq!(config.libraries, vec!["pthread"]);
        assert!(!config.cxx);
    }

    /// ds lists its sources across continuation lines with `dnl` comments
    /// between them, and declares a hard dependency on SPL.
    #[test]
    fn evaluates_continued_sources_with_interleaved_comments() {
        let script = "PHP_ARG_ENABLE(ds, whether to enable ds support,\n[  --enable-ds  Enable ds support])\n\nif test \"$PHP_DS\" != \"no\"; then\n  PHP_NEW_EXTENSION(ds,                       \\\n  src/common.c                                \\\n                                              \\\ndnl Internal\n  src/ds/ds_seq.c                      \\\n  php_ds.c                                        \\\n  , $ext_shared, -DZEND_ENABLE_STATIC_TSRMLS_CACHE=1)\n  PHP_ADD_EXTENSION_DEP(ds, spl)\nfi\n";
        let config = evaluate(script, &env("ds")).expect("evaluates");
        assert_eq!(config.sources, vec!["src/common.c", "src/ds/ds_seq.c", "php_ds.c"]);
        assert_eq!(config.dependencies, vec![ExtensionDependency { name: "spl".into(), optional: false }]);
        // ds passes its flag in the fourth position, `sapi_class`, which phpize
        // ignores for a shared extension; extra flags are the fifth.
        assert!(config.cflags.is_empty(), "{:?}", config.cflags);
    }

    /// zstd bundles its library behind `--with-libzstd`, which defaults to no:
    /// the bundled source variables must be the ones compiled.
    #[test]
    fn follows_the_bundled_branch_when_the_system_option_defaults_to_no() {
        let script = r#"
PHP_ARG_ENABLE(zstd, whether to enable zstd support, [  --enable-zstd  Enable zstd support])
PHP_ARG_WITH(libzstd, whether to use system zstd library, [  --with-libzstd  Use system zstd library], no, no)
if test "$PHP_ZSTD" != "no"; then
  if test "$PHP_LIBZSTD" != "no"; then
    AC_MSG_ERROR(pkg-config not found)
  else
    ZSTD_COMMON_SOURCES="
      zstd/lib/common/debug.c
      zstd/lib/common/xxhash.c
    "
    ZSTD_COMPRESS_SOURCES="zstd/lib/compress/hist.c"
    PHP_ADD_INCLUDE(PHP_EXT_SRCDIR(zstd)/zstd/lib/common)
  fi
  PHP_NEW_EXTENSION(zstd, zstd.c $ZSTD_COMMON_SOURCES $ZSTD_COMPRESS_SOURCES, $ext_shared)
fi
"#;
        let config = evaluate(script, &env("zstd")).expect("evaluates");
        assert_eq!(
            config.sources,
            vec!["zstd.c", "zstd/lib/common/debug.c", "zstd/lib/common/xxhash.c", "zstd/lib/compress/hist.c"]
        );
    }

    /// An extension that needs a library no target ships stops at its own
    /// AC_MSG_ERROR, and that message is what the user sees.
    #[test]
    fn a_missing_system_library_surfaces_the_extensions_own_error() {
        let script = r#"
PHP_ARG_WITH(yaml, whether to enable yaml, [ --with-yaml ])
if test "$PHP_YAML" != "no"; then
  PHP_CHECK_LIBRARY(yaml, yaml_parser_initialize, [
    PHP_ADD_LIBRARY(yaml)
  ], [
    AC_MSG_ERROR([Please install libyaml])
  ])
  PHP_NEW_EXTENSION(yaml, yaml.c, $ext_shared)
fi
"#;
        let error = evaluate(script, &env("yaml")).expect_err("libyaml is not a system library");
        assert!(error.contains("Please install libyaml"), "{error}");
    }

    /// Anything not modelled is noted, never silently guessed.
    #[test]
    fn unmodelled_macros_are_noted() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nif test \"$PHP_X\" != \"no\"; then\n  PHP_SETUP_OPENSSL(X_SHARED_LIBADD)\n  PHP_NEW_EXTENSION(x, x.c, $ext_shared)\nfi\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.notes.iter().any(|note| note.contains("PHP_SETUP_OPENSSL")), "{:?}", config.notes);
    }

    /// Verifies a script that never calls `PHP_NEW_EXTENSION` is an error mentioning that macro.
    #[test]
    fn a_script_without_php_new_extension_is_an_error() {
        let error = evaluate("PHP_ARG_ENABLE(x, x, x)\n", &env("x")).expect_err("nothing registered");
        assert!(error.contains("PHP_NEW_EXTENSION"));
    }

    /// Verifies `elif` and `else` branches are chosen correctly: only the matching branch's define
    /// is applied.
    #[test]
    fn elif_and_else_branches_are_honoured() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nMODE=b\nif test \"$MODE\" = \"a\"; then\n  AC_DEFINE(A)\nelif test \"$MODE\" = \"b\"; then\n  AC_DEFINE(B)\nelse\n  AC_DEFINE(C)\nfi\nPHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.keys().collect::<Vec<_>>(), vec!["B"]);
    }

    /// `m4_ifdef` takes the defined branch for macros phpize provides and the else branch for a
    /// third-party macro the extension did not ship.
    #[test]
    fn m4_ifdef_follows_what_phpize_defines() {
        let script = "PHP_ARG_ENABLE(x, x, x)\n\
            m4_ifdef([PHP_ADD_EXTENSION_DEP], [AC_DEFINE(KNOWN)], [AC_DEFINE(NOT_KNOWN)])\n\
            m4_ifdef([AX_THIRD_PARTY], [AC_DEFINE(THIRD)], [AC_DEFINE(NO_THIRD)])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.keys().collect::<Vec<_>>(), vec!["KNOWN", "NO_THIRD"]);
    }

    /// configure writes a define's value through a heredoc: a C string literal
    /// keeps its quotes, and only the UNQUOTED form expands variables.
    #[test]
    fn a_define_keeps_its_c_string_quotes() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nVER=2.0\n\
            AC_DEFINE([EXT_VERSION], [\"1.2.3\"], [Extension version])\n\
            AC_DEFINE([EXT_LITERAL], [\"$VER\"], [kept as written])\n\
            AC_DEFINE_UNQUOTED([EXT_BUILT], [\"$VER\"], [expanded])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.get("EXT_VERSION").map(String::as_str), Some("\"1.2.3\""));
        assert_eq!(config.defines.get("EXT_LITERAL").map(String::as_str), Some("\"$VER\""));
        assert_eq!(config.defines.get("EXT_BUILT").map(String::as_str), Some("\"2.0\""));
    }

    /// Test helper: an `Environment` whose extension directory is a fresh
    /// scratch directory holding `regular` (non-empty, executable), `empty`
    /// (zero bytes) and `subdir/`.
    fn env_with_files(tag: &str) -> Environment {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("elephc-config-m4-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subdir")).expect("scratch dir");
        std::fs::write(dir.join("regular"), "x").expect("regular file");
        std::fs::set_permissions(dir.join("regular"), std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::write(dir.join("empty"), "").expect("empty file");
        Environment { extension_dir: dir, ..env("x") }
    }

    /// Verifies a leading `!` negates an `if` condition, as in
    /// `if ! test -f x; then`.
    #[test]
    fn a_leading_bang_negates_the_condition() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nif ! test -f missing.h; then\n  AC_DEFINE(NEGATED)\nfi\nif ! test \"$PHP_X\" = \"yes\"; then\n  AC_DEFINE(WRONG)\nfi\nPHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.keys().collect::<Vec<_>>(), vec!["NEGATED"]);
    }

    /// Verifies `&&` and `||` between commands are evaluated as the shell does, left to right,
    /// and that a `test` the evaluator does not fully understand is refused rather than guessed.
    #[test]
    fn compound_conditions_use_every_command() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nENABLED=yes\nAVAILABLE=no\n\
            if test \"$ENABLED\" = yes && test \"$AVAILABLE\" = yes; then\n  AC_DEFINE(BOTH)\nfi\n\
            if test \"$ENABLED\" = no || test \"$AVAILABLE\" = no; then\n  AC_DEFINE(EITHER)\nfi\n\
            if [ \"$ENABLED\" = yes ] && ! test \"$AVAILABLE\" = yes; then\n  AC_DEFINE(NEGATED)\nfi\n\
            if test \"a&&b\" = \"a&&b\"; then\n  AC_DEFINE(QUOTED)\nfi\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.keys().collect::<Vec<_>>(), vec!["EITHER", "NEGATED", "QUOTED"]);
        let unknown = "PHP_ARG_ENABLE(x, x, x)\nif test \\( \"$PHP_X\" = yes \\); then\n  AC_DEFINE(GUESSED)\nfi\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let error = evaluate(unknown, &env("x")).expect_err("an unknown test is refused");
        assert!(error.contains("condition not understood"), "{error}");
    }

    /// Verifies non-ASCII text, in a comment inside an `if` and escaped by a backslash, is
    /// stepped over a whole character at a time instead of panicking mid-character.
    #[test]
    fn non_ascii_text_is_read_by_whole_characters() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nif test \"$PHP_X\" = yes; then\n  # café für ünïcode\n  \
            AC_DEFINE(INSIDE)\nfi\nNAME=caf\\é\nAC_DEFINE_UNQUOTED(NAME, [$NAME])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.defines.contains_key("INSIDE"));
        assert_eq!(config.defines.get("NAME").map(String::as_str), Some("café"));
    }

    /// Verifies a `&&`/`||` command line runs its macros as the shell would, `AC_MSG_ERROR` guards
    /// included, while a line holding a command the evaluator cannot run is skipped with a note.
    #[test]
    fn command_lists_run_their_macros_like_the_shell() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nFEATURE=yes\n\
            test \"$FEATURE\" = yes && AC_DEFINE(HAVE_FEATURE)\n\
            test \"$FEATURE\" = no && AC_DEFINE(NEVER)\n\
            test -z \"$FEATURE\" || EXTRA=on\n\
            AC_DEFINE_UNQUOTED(EXTRA, [$EXTRA])\n\
            pkg-config --exists libfoo || AC_DEFINE(PROBED)\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.defines.contains_key("HAVE_FEATURE"), "{:?}", config.defines);
        assert!(!config.defines.contains_key("NEVER"));
        assert_eq!(config.defines.get("EXTRA").map(String::as_str), Some("on"));
        assert!(!config.defines.contains_key("PROBED"), "an unrunnable probe leaves the line alone");
        assert!(config.notes.iter().any(|note| note.contains("pkg-config")), "{:?}", config.notes);
        // A list headed by an assignment or a macro call runs whole.
        let headed = "PHP_ARG_ENABLE(x, x, x)\nX=yes && Y=yes\n\
            AC_DEFINE(FIRST) && Z=$Y || W=no\n\
            AC_DEFINE_UNQUOTED(Y, [$Y])\nAC_DEFINE_UNQUOTED(Z, [$Z])\nAC_DEFINE_UNQUOTED(W, [${W:-unset}])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(headed, &env("x")).expect("evaluates");
        assert!(config.defines.contains_key("FIRST"), "{:?}", config.defines);
        assert_eq!(config.defines.get("Y").map(String::as_str), Some("yes"));
        assert_eq!(config.defines.get("Z").map(String::as_str), Some("yes"));
        assert_eq!(config.defines.get("W").map(String::as_str), Some("unset"));
        let guarded = "PHP_ARG_ENABLE(x, x, x)\nNEEDED=\n\
            test -n \"$NEEDED\" || AC_MSG_ERROR([libneeded is required])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let error = evaluate(guarded, &env("x")).expect_err("the guard fires");
        assert!(error.contains("libneeded is required"), "{error}");
    }

    /// Verifies every header of `AC_CHECK_HEADERS` is checked even after a missing one, and that
    /// only the plural `AC_CHECK_FUNCS` defines `HAVE_*`, as in autoconf.
    #[test]
    fn header_and_function_probes_define_like_autoconf() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nAC_CHECK_HEADERS([no_such_header.h stdint.h])\n\
            AC_CHECK_FUNC(strlcpy)\nAC_CHECK_FUNCS(memmove)\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.defines.contains_key("HAVE_STDINT_H"), "{:?}", config.defines);
        assert!(!config.defines.contains_key("HAVE_NO_SUCH_HEADER_H"));
        assert!(!config.defines.contains_key("HAVE_STRLCPY"));
        assert!(config.defines.contains_key("HAVE_MEMMOVE"));
    }

    /// Verifies a function the target lacks is not declared available: `AC_CHECK_FUNCS` probes
    /// each function, defines `HAVE_*` only for those that link, and runs the found or not-found
    /// action once per function.
    #[test]
    fn function_probes_follow_the_target() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nSEEN=\nAC_CHECK_FUNCS([mremap missing_mremap], [SEEN=\"$SEEN y\"], [SEEN=\"$SEEN n\"])\n\
            AC_CHECK_FUNC(missing_strlcpy, [AC_DEFINE(HAVE_IT, 1)], [AC_DEFINE(LACKS_IT, 1)])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let environment = env("x");
        let mut interpreter = Interpreter::new(&environment);
        interpreter.run(script, 0).expect("evaluates");
        let defines = &interpreter.config.defines;
        assert!(defines.contains_key("HAVE_MREMAP"), "{defines:?}");
        assert!(!defines.contains_key("HAVE_MISSING_MREMAP"));
        assert!(!defines.contains_key("HAVE_IT"));
        assert!(defines.contains_key("LACKS_IT"));
        assert_eq!(interpreter.vars.get("SEEN").map(|seen| seen.trim()), Some("y n"));
    }

    /// Verifies a function check links with the `LIBS`/`LDFLAGS` the script holds at that point,
    /// as configure's link test does, and that restoring them keeps the temporary flags out of
    /// the extension's link line.
    #[test]
    fn function_probes_link_with_the_current_libs() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nAC_CHECK_FUNC(cos, [], [AC_DEFINE(NO_COS_ALONE, 1)])\n\
            save_LIBS=\"$LIBS\"\nLIBS=\"$LIBS -lm\"\nLDFLAGS=\"-L/opt/m/lib\"\n\
            AC_CHECK_FUNC(cos, [AC_DEFINE(HAVE_COS_WITH_M, 1)], [AC_MSG_ERROR([missing cos])])\n\
            LIBS=\"$save_LIBS\"\nLDFLAGS=\nPHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let mut environment = env("x");
        environment.function_probe = FunctionProbe::new(|function, libraries, dirs| {
            function == "cos" && libraries.iter().any(|l| l == "m") && dirs.iter().any(|d| d == "/opt/m/lib")
        });
        let mut interpreter = Interpreter::new(&environment);
        interpreter.run(script, 0).expect("evaluates");
        let defines = &interpreter.config.defines;
        assert!(defines.contains_key("NO_COS_ALONE"), "{defines:?}");
        assert!(defines.contains_key("HAVE_COS_WITH_M"), "{defines:?}");
        interpreter.adopt_global_flags();
        assert!(interpreter.config.libraries.is_empty(), "{:?}", interpreter.config.libraries);
        assert!(interpreter.config.library_dirs.is_empty(), "{:?}", interpreter.config.library_dirs);
    }

    /// Verifies backslashes follow the shell: between double quotes and in the heredoc
    /// `AC_DEFINE_UNQUOTED` writes through, a backslash escapes only `\`, `$`, `` ` ``, `"` and a
    /// newline, so `\d` and `\n` keep it; an apostrophe there is an ordinary character; outside
    /// quotes every backslash escapes the next character.
    #[test]
    fn backslashes_and_apostrophes_follow_shell_quoting() {
        let script = "PHP_ARG_ENABLE(x, x, x)\n\
            RX=\"a\\db \\\"q\\\" it's \\\\\"\nBARE=a\\db\n\
            AC_DEFINE_UNQUOTED(TWO_LINES, [\"line1\\nline2\"])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let environment = env("x");
        let mut interpreter = Interpreter::new(&environment);
        interpreter.run(script, 0).expect("evaluates");
        assert_eq!(interpreter.vars.get("RX").map(String::as_str), Some("a\\db \"q\" it's \\"));
        assert_eq!(interpreter.vars.get("BARE").map(String::as_str), Some("adb"));
        assert_eq!(
            interpreter.config.defines.get("TWO_LINES").map(String::as_str),
            Some("\"line1\\nline2\"")
        );
    }

    /// Verifies header checks compile against the target instead of a fixed list: each header of
    /// `AC_CHECK_HEADERS` runs its own found or not-found action, and the include directories the
    /// script added (`PHP_ADD_INCLUDE`, `-I` in `CPPFLAGS`) reach the probe.
    #[test]
    fn header_probes_follow_the_target_one_header_at_a_time() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nSEEN=\n\
            AC_CHECK_HEADERS([sys/inotify.h missing_filio.h], [SEEN=\"$SEEN y\"], [SEEN=\"$SEEN n\"])\n\
            PHP_ADD_INCLUDE(vendor/include)\nCPPFLAGS=\"$CPPFLAGS -I/opt/q/include\"\n\
            AC_CHECK_HEADER(q.h, [AC_DEFINE(HAVE_Q, 1)], [AC_MSG_ERROR([q.h not found])])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let mut environment = env("x");
        environment.header_probe = HeaderProbe::new(|header, dirs| match header {
            "q.h" => {
                dirs.iter().any(|dir| dir == "/opt/q/include")
                    && dirs.iter().any(|dir| dir == "/nonexistent/ext/vendor/include")
            }
            other => !other.starts_with("missing_"),
        });
        let mut interpreter = Interpreter::new(&environment);
        interpreter.run(script, 0).expect("evaluates");
        let defines = &interpreter.config.defines;
        assert!(defines.contains_key("HAVE_SYS_INOTIFY_H"), "{defines:?}");
        assert!(!defines.contains_key("HAVE_MISSING_FILIO_H"));
        assert!(defines.contains_key("HAVE_Q"), "{defines:?}");
        assert_eq!(interpreter.vars.get("SEEN").map(|seen| seen.trim()), Some("y n"));
    }

    /// Verifies a library check links a call to its function with the library and the
    /// other-libraries argument, whatever library it names, instead of trusting a fixed list.
    #[test]
    fn library_probes_link_the_function_with_the_library() {
        let script = "PHP_ARG_ENABLE(x, x, x)\n\
            PHP_CHECK_LIBRARY(rt, shm_open, [PHP_ADD_LIBRARY(rt)], [AC_MSG_ERROR([no rt])])\n\
            AC_CHECK_LIB(z, inflate, [FOUND_Z=yes], [FOUND_Z=no], [-L/opt/z/lib])\n\
            AC_CHECK_LIB(z, missing_inflate, [MISS=yes], [MISS=no], [-L/opt/z/lib])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let mut environment = env("x");
        environment.function_probe = FunctionProbe::new(|function, libraries, dirs| {
            let has = |name: &str| libraries.iter().any(|library| library == name);
            match function {
                "shm_open" => has("rt"),
                "inflate" => has("z") && dirs.iter().any(|dir| dir == "/opt/z/lib"),
                _ => false,
            }
        });
        let mut interpreter = Interpreter::new(&environment);
        interpreter.run(script, 0).expect("evaluates");
        assert_eq!(interpreter.config.libraries, ["rt"]);
        assert_eq!(interpreter.vars.get("FOUND_Z").map(String::as_str), Some("yes"));
        assert_eq!(interpreter.vars.get("MISS").map(String::as_str), Some("no"));
    }

    /// Verifies `AC_CHECK_LIB` without a success action takes autoconf's default (link the library,
    /// define `HAVE_LIB<LIB>`), `AC_CHECK_SIZEOF` answers the LP64 sizes, and `CXXFLAGS` is kept
    /// for the C++ units.
    #[test]
    fn library_size_and_cxx_flag_probes_follow_autoconf() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nAC_CHECK_LIB(m, sqrt)\nAC_CHECK_LIB(pthread, pthread_create, [FOUND=yes])\n\
            AC_CHECK_SIZEOF(long)\nAC_CHECK_SIZEOF([void *])\nAC_CHECK_SIZEOF(long double)\n\
            CXXFLAGS=\"$CXXFLAGS -fvisibility=hidden\"\n\
            PHP_NEW_EXTENSION(x, x.cpp, $ext_shared,, , cxx)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.libraries.contains(&"m".to_string()), "{:?}", config.libraries);
        assert_eq!(config.defines.get("HAVE_LIBM").map(String::as_str), Some("1"));
        assert!(!config.libraries.contains(&"pthread".to_string()), "an explicit action replaces the default");
        assert!(!config.defines.contains_key("HAVE_LIBPTHREAD"));
        assert_eq!(config.defines.get("SIZEOF_LONG").map(String::as_str), Some("8"));
        assert_eq!(config.defines.get("SIZEOF_VOID_P").map(String::as_str), Some("8"));
        assert!(!config.defines.contains_key("SIZEOF_LONG_DOUBLE"), "its size differs between targets");
        assert_eq!(config.cxxflags, ["-fvisibility=hidden"]);
        assert!(!config.cflags.contains(&"-fvisibility=hidden".to_string()));
    }

    /// Verifies `${VAR:-default}` and its siblings apply their operator
    /// instead of yielding the bare variable.
    #[test]
    fn parameter_operators_apply_defaults_and_alternatives() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nEMPTY=\nSET=on\n\
            AC_DEFINE_UNQUOTED(A, [${UNSET:-fallback}])\n\
            AC_DEFINE_UNQUOTED(B, [${EMPTY:-fallback}])\n\
            AC_DEFINE_UNQUOTED(C, [${EMPTY-kept}x])\n\
            AC_DEFINE_UNQUOTED(D, [${SET:+alt}])\n\
            AC_DEFINE_UNQUOTED(E, [${UNSET:-${SET}}])\n\
            AC_DEFINE_UNQUOTED(F, [${SET:-fallback}])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        let define = |name: &str| config.defines.get(name).cloned().unwrap_or_default();
        assert_eq!(define("A"), "fallback");
        assert_eq!(define("B"), "fallback");
        assert_eq!(define("C"), "x");
        assert_eq!(define("D"), "alt");
        assert_eq!(define("E"), "on");
        assert_eq!(define("F"), "on");
    }

    /// Verifies the `test` file operators distinguish files, directories,
    /// executability and emptiness rather than all meaning "exists".
    #[test]
    fn file_test_operators_check_the_right_property() {
        let environment = env_with_files("file-tests");
        let mut script = String::from("PHP_ARG_ENABLE(x, x, x)\n");
        let cases = [
            ("F_REGULAR", "-f regular"),
            ("F_DIR", "-f subdir"),
            ("D_DIR", "-d subdir"),
            ("D_REGULAR", "-d regular"),
            ("X_REGULAR", "-x regular"),
            ("X_EMPTY", "-x empty"),
            ("R_REGULAR", "-r regular"),
            ("R_MISSING", "-r missing"),
            ("S_REGULAR", "-s regular"),
            ("S_EMPTY", "-s empty"),
        ];
        for (define, test) in cases {
            script.push_str(&format!("if test {test}; then\n  AC_DEFINE({define})\nfi\n"));
        }
        script.push_str("PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n");
        let config = evaluate(&script, &environment).expect("evaluates");
        let _ = std::fs::remove_dir_all(&environment.extension_dir);
        assert_eq!(
            config.defines.keys().collect::<Vec<_>>(),
            vec!["D_DIR", "F_REGULAR", "R_REGULAR", "S_REGULAR", "X_REGULAR"]
        );
    }

    /// Verifies the final `CFLAGS`/`CPPFLAGS`/`LIBS`/`LDFLAGS` values and
    /// `PHP_ADD_LIBRARY_WITH_PATH` reach the configuration, while a value a
    /// script saves and restores around a probe leaves nothing behind.
    #[test]
    fn global_flags_and_library_paths_reach_the_config() {
        let script = "PHP_ARG_ENABLE(x, x, x)\n\
            CFLAGS=\"$CFLAGS -DFROM_CFLAGS -I/opt/inc\"\n\
            CPPFLAGS=\"$CPPFLAGS -DFROM_CPPFLAGS\"\n\
            LIBS=\"$LIBS -lfoo -L/opt/lib\"\n\
            save_LDFLAGS=\"$LDFLAGS\"\n\
            LDFLAGS=\"$LDFLAGS -L/tmp/probe\"\n\
            LDFLAGS=\"$save_LDFLAGS\"\n\
            PHP_ADD_LIBRARY_WITH_PATH(bar, /opt/bar/lib, X_SHARED_LIBADD)\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert!(config.cflags.contains(&"-DFROM_CFLAGS".to_string()), "{:?}", config.cflags);
        assert!(config.cflags.contains(&"-DFROM_CPPFLAGS".to_string()), "{:?}", config.cflags);
        assert!(config.include_dirs.contains(&"/opt/inc".to_string()), "{:?}", config.include_dirs);
        assert_eq!(config.libraries, vec!["bar", "foo"]);
        assert_eq!(config.library_dirs, vec!["/opt/bar/lib", "/opt/lib"]);
    }

    /// Verifies `PHP_EVAL_LIBLINE`/`PHP_EVAL_INCLINE` split their flag lines
    /// and `PKG_CHECK_MODULES` is recorded in the notes instead of vanishing.
    #[test]
    fn flag_line_macros_are_modelled_and_pkg_config_is_noted() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nX_LIBS=\"-L/opt/x/lib -lx\"\nX_INCS=\"-I/opt/x/include -DX_ON\"\n\
            PHP_EVAL_LIBLINE($X_LIBS, X_SHARED_LIBADD)\n\
            PHP_EVAL_INCLINE($X_INCS)\n\
            PKG_CHECK_MODULES([LIBX], [libx >= 1.0])\n\
            PHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.libraries, vec!["x"]);
        assert_eq!(config.library_dirs, vec!["/opt/x/lib"]);
        assert_eq!(config.include_dirs, vec!["/opt/x/include"]);
        assert!(config.cflags.contains(&"-DX_ON".to_string()), "{:?}", config.cflags);
        assert!(config.notes.iter().any(|note| note.contains("PKG_CHECK_MODULES")), "{:?}", config.notes);
    }
}
