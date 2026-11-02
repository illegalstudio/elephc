//! The compile-time SAPI, published once per compilation and consulted by the front end.
//!
//! `--web` decides more than the process entry point. PHP's own front end behaves differently
//! per SAPI *before a single token is scanned*: the CLI removes a leading `#!` line from every
//! file it compiles, and a web SAPI emits those bytes as inline HTML like any other text before
//! `<?php`. Measured on php 8.5.10, on a file spelling `#!/usr/bin/env php` then
//! `<?php echo "hi\n";`:
//!
//! ```text
//!                       primary script      included file
//!   php file.php        hi                  hi              <- the line is gone
//!   php -S (both)       #!/usr/bin/env php  #!/usr/bin/env php
//!                       hi                  hi              <- the line is output
//! ```
//!
//! So the rule is not "the first file" -- it is the SAPI, for the whole compilation. Every PHP
//! CLI entry point carries that line (`bin/console`, `phpunit`, `composer`), which is why this is
//! a standard-PHP concern and not a framework one.
//!
//! Held thread-locally, like `source`'s parse mode, because one test process compiles both `--web`
//! and CLI programs and a process-global would let them overwrite each other. `pipeline::compile`
//! publishes it on the dedicated compile thread that then owns the whole front end.

use std::cell::Cell;

thread_local! {
    /// True while this thread compiles for a web SAPI; the CLI answer is the default.
    ///
    /// Defaulting to the CLI means a thread that never published anything -- a unit test lexing a
    /// snippet directly -- gets the answer `php file.php` would give, which is the one every such
    /// test is written against.
    static WEB_SAPI: Cell<bool> = const { Cell::new(false) };
}

/// Publishes the compile mode for this thread's front end.
pub(crate) fn set_web(web: bool) {
    WEB_SAPI.with(|flag| flag.set(web));
}

/// Returns the compile mode published for this thread's front end.
pub(crate) fn is_web() -> bool {
    WEB_SAPI.with(|flag| flag.get())
}

/// Whether a leading `#!` line is removed from every compiled file, as PHP's CLI does.
pub(crate) fn skips_leading_shebang() -> bool {
    WEB_SAPI.with(|flag| !flag.get())
}

/// Splits a leading shebang line off `source` when this compile mode removes it.
///
/// Returns the source to scan and the physical line its first byte sits on. PHP keeps the
/// numbering (a `throw` on physical line 3 of a shebang file reports line 3), so the bytes go but
/// the line counter does not: the scan restarts at line 2 over the remainder.
pub(crate) fn strip_leading_shebang(source: &str) -> (&str, usize) {
    if !source.starts_with("#!") || !skips_leading_shebang() {
        return (source, 1);
    }
    match source.find('\n') {
        // The newline goes with the line: php prints `PLAIN\nhi\n`, not `\nPLAIN\nhi\n`, for a
        // shebang followed by inline text.
        Some(end) => (&source[end + 1..], 2),
        // A file that is nothing but a shebang compiles to an empty program.
        None => ("", 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the CLI answer removes the whole line, newline included, and moves to line 2.
    #[test]
    fn cli_removes_the_shebang_line_and_keeps_the_numbering() {
        set_web(false);
        assert_eq!(
            strip_leading_shebang("#!/usr/bin/env php\n<?php\n"),
            ("<?php\n", 2)
        );
        assert_eq!(strip_leading_shebang("#!/usr/bin/env php"), ("", 1));
    }

    /// Verifies a web compile leaves the bytes alone, so they are echoed as inline HTML.
    #[test]
    fn a_web_compile_keeps_the_shebang_bytes() {
        set_web(true);
        let source = "#!/usr/bin/env php\n<?php\n";
        assert_eq!(strip_leading_shebang(source), (source, 1));
        set_web(false);
    }

    /// Verifies only a leading `#!` is a shebang: the same spelling later in the file is a comment.
    #[test]
    fn only_the_first_two_bytes_make_a_shebang() {
        set_web(false);
        let source = "<?php\n#!/usr/bin/env php\n";
        assert_eq!(strip_leading_shebang(source), (source, 1));
    }
}
