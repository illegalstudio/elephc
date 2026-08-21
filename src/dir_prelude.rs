//! Purpose:
//! PHP's object directory surface — the `Directory` class and the `dir()` function that mints one
//! — implemented in elephc-PHP on top of the existing `opendir`/`readdir`/`rewinddir`/`closedir`
//! builtins. `dir()` is the only way php hands out a `Directory`, and both were absent: a program
//! calling `dir($path)` failed with "Undefined function".
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after include
//!   resolution and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Every method here is one existing builtin call, so the
//!   whole surface compiles through the ordinary class/function pipeline with NO new assembly —
//!   both architectures get it at once. A Rust-declared class would instead need its name added to
//!   the eight hardcoded builtin-class lists AND its methods allow-listed in EIR lowering, for a
//!   class whose bodies are one-liners.
//! - PAY-FOR-USE. Injected only when `detect::program_uses_directory` finds a reference, so a
//!   program that never opens a directory object carries neither the class nor the function.
//! - WHY THE CONSTRUCTOR IS PUBLIC AND STILL REFUSES. php reports
//!   `Error: Cannot directly construct Directory, use dir() instead` — a message a `private
//!   function __construct()` cannot produce, since php's own wording for that is
//!   "Call to private Directory::__construct()". The static gate reproduces php's message exactly
//!   while still letting `dir()` build one. It is not re-entrancy-safe by construction, but the
//!   window spans a single `new` with no user code in it.
//! - EVERY method BINDS `$this->handle` TO A LOCAL before calling. This is not style: passing a
//!   `mixed` object property inline as a call argument trips a pre-existing codegen leak (see the
//!   long note in `crate::hash_prelude`), costing one heap block per call.
//! - `$context` is accepted and IGNORED, the way `mkdir()`/`rmdir()`/`opendir()` already accept
//!   it. Refusing the argument outright would make `dir($p, $ctx)` a compile error on a signature
//!   php documents.
//! - MEASURED DIVERGENCES against php 8.5.6, both inherited from `opendir()` rather than added
//!   here: php warns `dir(<path>): Failed to open directory: No such file or directory` for a
//!   directory it cannot open, where elephc's `opendir()` is silent and so this is too; and php's
//!   methods throw `TypeError: Directory::read(): cannot use Directory resource after it has been
//!   closed`, where the underlying builtin reports the closed handle in its own words. The VALUES
//!   — `Directory|false`, `$path`, `$handle`, the readdir-order listing, `read()`'s `string|false`,
//!   `rewind()`/`close()` returning null — all match.

mod detect;

/// The elephc-PHP directory prelude: the `Directory` class and the `dir()` function.
pub(crate) const DIR_PRELUDE_SRC: &str = r#"<?php

final class Directory {
    public string $path = '';
    public mixed $handle = null;
    public static bool $__elephc_opening = false;

    public function __construct() {
        if (!Directory::$__elephc_opening) {
            throw new \Error('Cannot directly construct Directory, use dir() instead');
        }
    }

    public function read(): string|false {
        $handle = $this->handle;
        return readdir($handle);
    }

    public function rewind(): void {
        $handle = $this->handle;
        rewinddir($handle);
    }

    public function close(): void {
        $handle = $this->handle;
        closedir($handle);
    }
}

function dir(string $directory, mixed $context = null): Directory|false {
    $handle = opendir($directory);
    if ($handle === false) {
        return false;
    }
    Directory::$__elephc_opening = true;
    $entry = new Directory();
    Directory::$__elephc_opening = false;
    $entry->path = $directory;
    $entry->handle = $handle;
    return $entry;
}
"#;

/// Injects the directory prelude when the program references `dir()` or `Directory`, leaving every
/// other program untouched.
///
/// A program that declares its OWN `dir` function or `Directory` class suppresses injection, so
/// adding this prelude can never turn a working program into a redeclaration error. The prelude
/// carries only declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(program: crate::parser::ast::Program) -> crate::parser::ast::Program {
    if !detect::program_uses_directory(&program) || detect::program_declares_directory(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(DIR_PRELUDE_SRC).expect("dir prelude must tokenize");
    let mut combined = crate::parser::parse_internal(&tokens).expect("dir prelude must parse");
    combined.extend(program);
    combined
}
