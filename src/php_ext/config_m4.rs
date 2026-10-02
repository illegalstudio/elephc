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
//! - Autoconf probes (`AC_RUN_IFELSE`, `AC_CHECK_FUNCS`, …) are answered as a
//!   modern POSIX system would, since every supported target is one. A library
//!   probe succeeds only for libraries every target ships (`pthread`, `m`,
//!   `dl`); any other takes its failure branch, which is where an extension
//!   reports a missing dependency with `AC_MSG_ERROR` — surfaced verbatim.
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
    /// True when any unit must be compiled as C++.
    pub cxx: bool,
    /// `config.h` defines, by name.
    pub defines: BTreeMap<String, String>,
    /// Include directories, relative to the extension directory or absolute.
    pub include_dirs: Vec<String>,
    pub dependencies: Vec<ExtensionDependency>,
    /// System libraries the extension links (`PHP_ADD_LIBRARY`).
    pub libraries: Vec<String>,
    /// Constructs that were not modelled, kept for diagnostics.
    pub notes: Vec<String>,
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
}

/// Evaluates `text` and returns the extension's build configuration, or the
/// message of the `AC_MSG_ERROR` the script reached.
pub fn evaluate(text: &str, env: &Environment) -> Result<ExtensionConfig, String> {
    let mut interpreter = Interpreter::new(env);
    interpreter.run(&strip_dnl(text), 0)?;
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
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
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
            && text[i..].starts_with("dnl")
            && bytes.get(i + 3).is_none_or(|b| !is_word_byte(*b))
        {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Libraries every supported target provides, so a probe for them succeeds.
const SYSTEM_LIBRARIES: &[&str] = &["pthread", "m", "dl", "c"];

/// Headers every supported target provides; a probe for any other fails.
const SYSTEM_HEADERS: &[&str] = &[
    "stdint.h", "inttypes.h", "stdlib.h", "string.h", "strings.h", "unistd.h", "stdio.h",
    "stdarg.h", "stddef.h", "stdbool.h", "limits.h", "errno.h", "fcntl.h", "signal.h",
    "time.h", "pthread.h", "sched.h", "dirent.h", "sys/types.h", "sys/stat.h", "sys/time.h",
    "sys/mman.h", "sys/file.h", "sys/ipc.h", "sys/shm.h", "sys/socket.h", "sys/param.h",
    "sys/resource.h", "sys/wait.h", "netinet/in.h", "arpa/inet.h", "netdb.h", "poll.h",
    "math.h", "locale.h", "wchar.h", "ctype.h", "assert.h", "semaphore.h",
];

struct Interpreter<'a> {
    env: &'a Environment,
    vars: HashMap<String, String>,
    config: ExtensionConfig,
}

impl<'a> Interpreter<'a> {
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
        Self { env, vars, config: ExtensionConfig::default() }
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
                    if self.condition(condition) {
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
            Statement::Command => Ok(()),
        }
    }

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
                if !library.is_empty() && !self.config.libraries.contains(&library) {
                    self.config.libraries.push(library);
                }
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
                    if self.condition(arg(index)) {
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
            "AC_CHECK_FUNCS" | "AC_CHECK_FUNC" => {
                for function in self.expand(arg(0)).split_whitespace() {
                    self.config
                        .defines
                        .insert(format!("HAVE_{}", function.to_ascii_uppercase()), "1".to_string());
                }
                self.run(arg(1), depth + 1)?;
            }
            "AC_CHECK_HEADERS" | "AC_CHECK_HEADER" => {
                let headers = self.expand(arg(0));
                let all_present = headers.split_whitespace().all(|header| {
                    let present = SYSTEM_HEADERS.contains(&header);
                    if present && name == "AC_CHECK_HEADERS" {
                        let define = format!(
                            "HAVE_{}",
                            header.to_ascii_uppercase().replace(['/', '.', '-'], "_")
                        );
                        self.config.defines.insert(define, "1".to_string());
                    }
                    present
                });
                self.run(if all_present { arg(1) } else { arg(2) }, depth + 1)?;
            }
            "AC_CHECK_LIB" | "PHP_CHECK_LIBRARY" => {
                let library = self.expand(arg(0)).trim().to_string();
                if SYSTEM_LIBRARIES.contains(&library.as_str()) {
                    self.run(arg(2), depth + 1)?;
                } else {
                    self.note(format!("library probe for '{library}' answered 'not found'"));
                    self.run(arg(3), depth + 1)?;
                }
            }
            "AC_C_BIGENDIAN" => self.run(arg(1), depth + 1)?,
            "ifdef" | "m4_ifdef" => self.run(arg(1), depth + 1)?,
            "AC_MSG_ERROR" | "AC_MSG_FAILURE" => {
                return Err(format!("config.m4 stopped: {}", self.expand(arg(0)).trim()));
            }
            // Output, bookkeeping, and build-system plumbing phpize owns.
            "AC_MSG_CHECKING" | "AC_MSG_RESULT" | "AC_MSG_NOTICE" | "AC_MSG_WARN"
            | "PHP_SUBST" | "PHP_ADD_MAKEFILE_FRAGMENT" | "PHP_INSTALL_HEADERS" | "PHP_ADD_BUILD_DIR"
            | "AC_LANG_PUSH" | "AC_LANG_POP" | "AC_LANG_PROGRAM" | "AC_LANG_SOURCE" | "AC_REQUIRE"
            | "AC_PROG_CC" | "AC_PROG_CXX" | "AC_CONFIG_HEADERS" | "PHP_EVAL_LIBLINE"
            | "PHP_EVAL_INCLINE" | "AC_CHECK_TYPES" | "AC_CHECK_SIZEOF" | "AC_CHECK_DECLS"
            | "AC_CHECK_DECL" | "AC_PATH_PROG" | "PKG_CHECK_MODULES" | "AC_SUBST" | "AS_HELP_STRING"
            | "PHP_ALWAYS_SHARED" | "AC_CONFIG_FILES" | "AC_OUTPUT" => {}
            other => self.note(format!("ignored macro {other}")),
        }
        Ok(())
    }

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

    fn add_cflags(&mut self, flags: &str) {
        for flag in self.expand(flags).split_whitespace() {
            self.config.cflags.push(flag.trim_matches('"').to_string());
        }
    }

    /// Expands `$VAR`, `${VAR}` and backtick commands in shell text, and drops
    /// the quote characters themselves.
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
        self.expand(&shielded)
            .replace(ESCAPED_QUOTE, "\\\"")
            .replace(QUOTE, "\"")
            .replace(APOSTROPHE, "'")
            .trim()
            .to_string()
    }

    fn expand(&self, text: &str) -> String {
        let text = &self.expand_dir_macros(text);
        let bytes = text.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                // `\$(VAR)` defers a variable to make; the value is the same.
                b'\\' if bytes.get(i + 1) == Some(&b'$') => i += 1,
                b'\\' if i + 1 < bytes.len() => {
                    if bytes[i + 1] != b'\n' {
                        out.push(bytes[i + 1] as char);
                    } else {
                        out.push(' ');
                    }
                    i += 2;
                }
                b'"' => i += 1,
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
                        let end = inner.find('}').unwrap_or(inner.len());
                        let name = inner[..end].split([':', '-', '=']).next().unwrap_or("");
                        out.push_str(self.vars.get(name).map(String::as_str).unwrap_or(""));
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

    /// Evaluates a shell condition: `test …`, `[ … ]`, or anything else (false).
    fn condition(&self, text: &str) -> bool {
        let text = text.trim().trim_end_matches(';').trim();
        let body = if let Some(rest) = text.strip_prefix("test ") {
            rest
        } else if let Some(rest) = text.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            rest
        } else {
            return false;
        };
        let words = self.words(body);
        let mut position = 0;
        self.test_or(&words, &mut position)
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

    fn test_or(&self, words: &[String], position: &mut usize) -> bool {
        let mut result = self.test_and(words, position);
        while words.get(*position).map(String::as_str) == Some("-o") {
            *position += 1;
            let right = self.test_and(words, position);
            result = result || right;
        }
        result
    }

    fn test_and(&self, words: &[String], position: &mut usize) -> bool {
        let mut result = self.test_term(words, position);
        while words.get(*position).map(String::as_str) == Some("-a") {
            *position += 1;
            let right = self.test_term(words, position);
            result = result && right;
        }
        result
    }

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
                    match first.as_str() {
                        "-d" => path.is_dir(),
                        _ => path.exists(),
                    }
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

/// One statement of the script, borrowing from its text.
enum Statement<'s> {
    Macro { name: &'s str, args: Vec<String> },
    Assign { name: &'s str, value: &'s str },
    If { arms: Vec<(&'s str, &'s str)>, otherwise: Option<&'s str> },
    Skipped(&'s str),
    Command,
}

/// Reads statements from shell + m4 text.
struct Cursor<'s> {
    text: &'s str,
    position: usize,
}

impl<'s> Cursor<'s> {
    fn new(text: &'s str) -> Self {
        Self { text, position: 0 }
    }

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

    fn word_at(&self, at: usize) -> &'s str {
        let bytes = self.bytes();
        let len = bytes[at..].iter().take_while(|b| is_word_byte(**b)).count();
        &self.text[at..at + len]
    }

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
        if !word.is_empty() && bytes.get(after) == Some(&b'(') {
            let (args, end) = read_macro_args(self.text, after + 1);
            self.position = end;
            return Some(Statement::Macro { name: word, args });
        }
        if !word.is_empty() && bytes.get(after) == Some(&b'=') {
            let value_end = self.shell_word_end(after + 1);
            self.position = value_end;
            return Some(Statement::Assign { name: word, value: &self.text[after + 1..value_end] });
        }
        let end = self.command_end(start);
        self.position = end.max(start + 1);
        Some(Statement::Command)
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

    /// Moves past one byte, or a whole quoted/bracketed span.
    fn advance(&self, i: usize) -> usize {
        let bytes = self.bytes();
        match bytes[i] {
            b'"' | b'\'' | b'`' => skip_quoted(bytes, i),
            b'[' => skip_bracketed(bytes, i),
            b'\\' => i + 2,
            _ => i + 1,
        }
    }

    /// The keyword starting at `i`, if `i` begins a word in command position.
    fn keyword_at(&self, i: usize) -> Option<&'s str> {
        let bytes = self.bytes();
        if i > 0 && is_word_byte(bytes[i - 1]) {
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

    fn previous_word(&self, i: usize) -> Option<&'s str> {
        let before = self.text[..i].trim_end();
        let start = before.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map(|p| p + 1).unwrap_or(0);
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

fn finish_arg(raw: &str) -> String {
    raw.trim_start().trim_end_matches([' ', '\t', '\n', '\r', '\\']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(extension: &str) -> Environment {
        Environment {
            extension: extension.to_string(),
            extension_dir: PathBuf::from("/nonexistent/ext"),
            php_include_dir: PathBuf::from("/nonexistent/include"),
            php_version: "8.5.6".to_string(),
        }
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

    #[test]
    fn a_script_without_php_new_extension_is_an_error() {
        let error = evaluate("PHP_ARG_ENABLE(x, x, x)\n", &env("x")).expect_err("nothing registered");
        assert!(error.contains("PHP_NEW_EXTENSION"));
    }

    #[test]
    fn elif_and_else_branches_are_honoured() {
        let script = "PHP_ARG_ENABLE(x, x, x)\nMODE=b\nif test \"$MODE\" = \"a\"; then\n  AC_DEFINE(A)\nelif test \"$MODE\" = \"b\"; then\n  AC_DEFINE(B)\nelse\n  AC_DEFINE(C)\nfi\nPHP_NEW_EXTENSION(x, x.c, $ext_shared)\n";
        let config = evaluate(script, &env("x")).expect("evaluates");
        assert_eq!(config.defines.keys().collect::<Vec<_>>(), vec!["B"]);
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
}
