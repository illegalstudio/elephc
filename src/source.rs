//! Purpose:
//! Defines physical-source loading and the per-file source profile: the language mode selected
//! from a path plus the `declare(strict_types=1)` state that file opted into. Centralizes byte
//! decoding and `.lfc` classification so every loader agrees on source semantics.
//!
//! Called from:
//! - `crate::pipeline::compile()` for the entry source.
//! - `crate::resolver` and `crate::autoload` for additional physical source files.
//!
//! Key details:
//! - Only `.lfc` opts into tagless elephc source; every other path preserves tagged-PHP behavior.
//! - Classification is ASCII case-insensitive and never changes output-path naming.
//! - `strict_types` is a *per-file* PHP directive. It is stamped onto every `Stmt` created while
//!   one physical file is parsed (`crate::parser::ast::Stmt::strict_types`) and therefore survives
//!   include/autoload merging into the single flat program the type checker sees. Statement
//!   rewriting passes must re-install the profile they read off the statement they are rebuilding,
//!   which is why `with_parse_mode`/`scoped_parse_mode` take the whole `SourceProfile` instead of
//!   the mode alone: a rebuild that dropped the flag would silently downgrade a strict file to
//!   PHP's coercive parameter binding.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;

use crate::errors::CompileError;
use crate::parser::ast::Program;

/// Language profile selected for one physical source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceMode {
    /// Tagged PHP-compatible source that requires the normal `<?php` opening tag.
    Php,
    /// Tagless elephc source with every elephc extension available.
    Lfc,
    /// Compiler-generated source that is never subject to the user strict-PHP audit.
    Internal,
}

/// Everything one physical source file contributes to the AST nodes parsed from it.
///
/// `mode` comes from the file's path and is known before parsing starts; `strict_types` comes
/// from a `declare(strict_types=1)` directive and is only known once the parser has read the
/// file's first statement. Both are stamped onto every `Stmt` the file produces, so the merged
/// program still answers "which file was this written in" for the two questions that need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceProfile {
    /// Language profile selected from the physical path.
    pub mode: SourceMode,
    /// Whether the file declared `strict_types=1`.
    pub strict_types: bool,
}

impl SourceProfile {
    /// Builds the profile a physical file starts parsing with: its path-derived mode and PHP's
    /// default coercive typing, which only a `declare(strict_types=1)` directive changes.
    pub fn new(mode: SourceMode) -> Self {
        Self {
            mode,
            strict_types: false,
        }
    }
}

thread_local! {
    /// Source mode inherited by AST nodes created during one parser invocation.
    static CURRENT_PARSE_MODE: Cell<SourceMode> = const { Cell::new(SourceMode::Internal) };

    /// `strict_types` state inherited by AST nodes created during one parser invocation.
    static CURRENT_STRICT_TYPES: Cell<bool> = const { Cell::new(false) };
}

impl SourceMode {
    /// Classifies a physical path, treating only a case-insensitive `.lfc` suffix as tagless.
    pub fn from_path(path: &Path) -> Self {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lfc"))
        {
            Self::Lfc
        } else {
            Self::Php
        }
    }

    /// Returns whether this source mode requires a user-written `<?php` opening tag.
    pub fn requires_open_tag(self) -> bool {
        matches!(self, Self::Php)
    }

    /// Returns whether an invocation-level strict-PHP request applies to this source.
    pub fn strict_php_is_effective(self, requested: bool) -> bool {
        requested && matches!(self, Self::Php)
    }
}

/// Returns whether static source discovery should inspect a path as PHP/LFC source.
///
/// Physical includes remain PHP-compatible regardless of suffix, while directory-based
/// discovery intentionally indexes only files named `.php` or `.lfc`.
pub fn is_discoverable_source_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("php") || extension.eq_ignore_ascii_case("lfc")
        })
}

/// Removes the recognized source suffix from one discovered path component.
pub fn discoverable_source_stem(component: &str) -> String {
    Path::new(component)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(component)
        .to_string()
}

/// Reads PHP source without requiring its byte stream to be valid UTF-8.
///
/// PHP identifiers and string literals are BYTE strings: bytes in `0x80..=0xff` are legal in both,
/// and third-party packages use that. `symfony/cache` declares `class \xA9` -- one non-ASCII byte,
/// no namespace -- and Composer's generated classmap carries the same byte as a key.
///
/// A malformed byte goes through `crate::string_bytes`, the SAME private-use marker range a
/// `\xNN` escape already uses, so a raw source byte and its escaped spelling produce identical
/// runtime bytes. Latin-1 was used here before and is not reversible: byte `0xA9` became
/// `U+00A9`, which re-encodes to TWO bytes, and every byte operation on it was then wrong --
/// measured against php 8.5.10, `strlen("\xA9")` answered 2 against 1, `ord()` 194 against 169,
/// `bin2hex()` `c2a9` against `a9`. Internal coherence was preserved, which is why it went
/// unnoticed: both sides of a comparison were transformed alike, and only the bytes LEAVING the
/// program differed.
pub(crate) fn read_physical_source(path: impl AsRef<Path>) -> std::io::Result<String> {
    read_physical_bytes(path).map(|bytes| decode_physical_source(bytes.to_vec()))
}

/// One file's bytes and what identifies that version of it.
struct RememberedSource {
    len: u64,
    modified: Option<std::time::SystemTime>,
    bytes: std::rc::Rc<[u8]>,
}

thread_local! {
    /// The sources this compile already read. The closure of a Symfony build is walked several
    /// times (the prelude probe, the real load, the preload units), so each of about 30 000 files
    /// was opened three times, and opening was a tenth of the whole build.
    static READ_SOURCES: std::cell::RefCell<crate::fast_hash::FastMap<std::path::PathBuf, RememberedSource>> =
        std::cell::RefCell::new(Default::default());
    /// `canonical` answers of this compile; each one is a `realpath` walking every component.
    static CANONICAL_PATHS: std::cell::RefCell<crate::fast_hash::FastMap<std::path::PathBuf, std::path::PathBuf>> =
        std::cell::RefCell::new(Default::default());
}

#[cfg(test)]
thread_local! {
    /// Files actually opened by [`read_physical_bytes`].
    pub(crate) static SOURCE_OPENS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Forgets what this thread read; a compile starts from what is on disk.
pub(crate) fn reset_source_memos() {
    READ_SOURCES.with(|memo| memo.borrow_mut().clear());
    CANONICAL_PATHS.with(|memo| memo.borrow_mut().clear());
}

/// The bytes of a file, read once per version of it (size and modification time).
pub(crate) fn read_physical_bytes(path: impl AsRef<Path>) -> std::io::Result<std::rc::Rc<[u8]>> {
    let path = path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    let known = READ_SOURCES.with(|memo| {
        memo.borrow()
            .get(path)
            .filter(|source| source.len == len && source.modified == modified)
            .map(|source| std::rc::Rc::clone(&source.bytes))
    });
    if let Some(bytes) = known {
        return Ok(bytes);
    }
    #[cfg(test)]
    SOURCE_OPENS.with(|opens| opens.set(opens.get() + 1));
    let bytes: std::rc::Rc<[u8]> = std::fs::read(path)?.into();
    READ_SOURCES.with(|memo| {
        memo.borrow_mut().insert(
            path.to_path_buf(),
            RememberedSource { len, modified, bytes: std::rc::Rc::clone(&bytes) },
        );
    });
    Ok(bytes)
}

/// `Path::canonicalize`, answered once per path during a compile. A failure is not remembered.
pub(crate) fn canonical(path: impl AsRef<Path>) -> std::io::Result<std::path::PathBuf> {
    let path = path.as_ref();
    if let Some(known) = CANONICAL_PATHS.with(|memo| memo.borrow().get(path).cloned()) {
        return Ok(known);
    }
    let resolved = path.canonicalize()?;
    CANONICAL_PATHS.with(|memo| memo.borrow_mut().insert(path.to_path_buf(), resolved.clone()));
    Ok(resolved)
}

/// Converts one physical PHP byte stream into the UTF-8 storage used by the parser.
fn decode_physical_source(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        // A valid UTF-8 source still needs normalising IF it spells a private-use char in the
        // marker range itself, or that char would be indistinguishable from the marker a malformed
        // byte decodes to. The guard keeps the rebuild off the hot path: essentially no real
        // source contains one, and the check is a scan rather than an allocation.
        Ok(source) if !holds_byte_marker(&source) => source,
        Ok(source) => {
            let mut normalised = String::with_capacity(source.len());
            push_source_text(&source, &mut normalised);
            normalised
        }
        Err(error) => decode_mixed_utf8_and_latin1(error.into_bytes()),
    }
}

/// Whether `source` spells a char in the byte-marker range, which only this decode may produce.
///
/// A BYTE scan, not a char walk: every marker in `U+E000..=U+E0FF` encodes as three UTF-8 bytes
/// beginning `0xEE`, so a source without that byte cannot hold one and pays a memchr-shaped pass
/// instead of a full decode. This runs on every source file the compiler reads, and the char walk
/// it replaces was heavy enough to push a cross-process lease test past its timeout.
fn holds_byte_marker(source: &str) -> bool {
    source
        .as_bytes()
        .contains(&0xee)
        .then(|| {
            source
                .chars()
                .any(|ch| ('\u{e000}'..='\u{e0ff}').contains(&ch))
        })
        .unwrap_or(false)
}

/// Appends a VALID UTF-8 span, routing it through the byte-marker encoder.
///
/// Not optional: a source that genuinely spells a private-use char in `U+E000..=U+E0FF` would
/// otherwise be indistinguishable from a marker this decode produced, and
/// `string_bytes::push_literal_char` is exactly the escape hatch for that collision.
fn push_source_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        crate::string_bytes::push_literal_char(ch, out);
    }
}

/// Preserves valid UTF-8 spans while mapping each malformed byte to its byte marker.
fn decode_mixed_utf8_and_latin1(bytes: Vec<u8>) -> String {
    let mut source = String::with_capacity(bytes.len());
    let mut offset = 0;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                push_source_text(valid, &mut source);
                break;
            }
            Err(error) => {
                let valid_end = offset + error.valid_up_to();
                if valid_end > offset {
                    let valid = std::str::from_utf8(&bytes[offset..valid_end])
                        .expect("UTF-8 validator reported a valid prefix");
                    push_source_text(valid, &mut source);
                }
                offset = valid_end;
                let invalid_len = error.error_len().unwrap_or(bytes.len() - offset);
                for byte in &bytes[offset..offset + invalid_len] {
                    crate::string_bytes::push_escaped_byte(*byte, &mut source);
                }
                offset += invalid_len;
            }
        }
    }
    source
}

/// RAII guard restoring the parser's previous source profile on drop.
pub(crate) struct ParseModeGuard {
    previous: SourceProfile,
}

impl Drop for ParseModeGuard {
    /// Restores the parser source profile active before the nested parse.
    fn drop(&mut self) {
        CURRENT_PARSE_MODE.with(|cell| cell.set(self.previous.mode));
        CURRENT_STRICT_TYPES.with(|cell| cell.set(self.previous.strict_types));
    }
}

/// Runs `f` while parser-created AST nodes inherit `profile`.
pub(crate) fn with_parse_mode<T>(profile: SourceProfile, f: impl FnOnce() -> T) -> T {
    let _guard = scoped_parse_mode(profile);
    f()
}

/// Installs one parser/source reconstruction profile until the returned guard is dropped.
pub(crate) fn scoped_parse_mode(profile: SourceProfile) -> ParseModeGuard {
    let mode = CURRENT_PARSE_MODE.with(|cell| cell.replace(profile.mode));
    let strict_types = CURRENT_STRICT_TYPES.with(|cell| cell.replace(profile.strict_types));
    ParseModeGuard {
        previous: SourceProfile { mode, strict_types },
    }
}

/// Returns the source mode assigned to AST nodes created at the current parse site.
pub(crate) fn current_parse_mode() -> SourceMode {
    CURRENT_PARSE_MODE.with(Cell::get)
}

/// Returns the `strict_types` state assigned to AST nodes created at the current parse site.
pub(crate) fn current_strict_types() -> bool {
    CURRENT_STRICT_TYPES.with(Cell::get)
}

/// Records that the file currently being parsed declared `strict_types=<enabled>`.
///
/// PHP requires the directive to be a file's very first statement, so every statement created
/// after this call belongs to the same file and inherits the flag. The enclosing
/// `ParseModeGuard` resets it when the file's parse ends, which is what keeps the directive from
/// leaking into an included file or back out to the includer.
pub(crate) fn declare_strict_types(enabled: bool) {
    CURRENT_STRICT_TYPES.with(|cell| cell.set(enabled));
}

/// Applies path-dependent post-parse processing shared by every physical source loader.
///
/// Magic constants retain the real path, strict PHP audits the unfiltered physical
/// program, and conditional compilation runs last so inactive extension syntax in a
/// PHP file cannot evade the audit.
pub fn finalize_physical_program(
    program: Program,
    source: &str,
    path: &Path,
    mode: SourceMode,
    defines: &HashSet<String>,
) -> Result<Program, CompileError> {
    let program = crate::magic_constants::substitute_file_and_scope_constants(program, path);
    crate::strict_php::check_file_with_mode(&program, &path.display().to_string(), mode)?;
    // Generic doc-comment annotations are applied AFTER the strict audit, and that order is the
    // whole point of the surface: a `@template` file is valid PHP — the annotations are
    // comments — so `--strict-php` must keep accepting it. Applying them first made the audit
    // see the injected `<T>` and `array<T>` and reject a file php-src parses happily.
    //
    // Per physical file, because the lexer discards comments and they are recovered from the
    // source text by line: that only means anything while the file is still its own, before
    // include resolution splices every file into one program without rebasing line numbers.
    let program = crate::docblock::erasure::apply_for_file(program, source, path);
    Ok(crate::conditional::apply(program, defines))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `.lfc` classification is case-insensitive while other suffixes remain PHP mode.
    #[test]
    fn classifies_only_lfc_as_tagless() {
        assert_eq!(SourceMode::from_path(Path::new("main.lfc")), SourceMode::Lfc);
        assert_eq!(SourceMode::from_path(Path::new("main.LFC")), SourceMode::Lfc);
        assert_eq!(SourceMode::from_path(Path::new("main.php")), SourceMode::Php);
        assert_eq!(SourceMode::from_path(Path::new("bootstrap.inc")), SourceMode::Php);
        assert_eq!(SourceMode::from_path(Path::new("script")), SourceMode::Php);
    }

    /// Verifies directory-based discovery accepts only the two physical source suffixes.
    #[test]
    fn classifies_discoverable_source_paths() {
        assert!(is_discoverable_source_path(Path::new("src/App.php")));
        assert!(is_discoverable_source_path(Path::new("src/App.LFC")));
        assert!(!is_discoverable_source_path(Path::new("src/App.inc")));
        assert_eq!(discoverable_source_stem("App.lfc"), "App");
    }

    /// Verifies physical source keeps valid Unicode while accepting isolated PHP identifier bytes.
    ///
    /// The isolated byte becomes a `string_bytes` MARKER, not the Latin-1 char it used to become.
    /// That is the whole point: a marker decodes back to ONE byte, while `U+00A9` re-encodes to
    /// two and made `strlen`, `ord` and `bin2hex` disagree with php on any source holding a raw
    /// high byte. The valid `é` is untouched, so nothing that was already UTF-8 moves.
    #[test]
    fn decodes_mixed_utf8_and_php_identifier_bytes() {
        let source = decode_physical_source(vec![b'e', 0xc3, 0xa9, b' ', 0xa9, b'!']);
        let mut expected = String::from("eé ");
        crate::string_bytes::push_escaped_byte(0xa9, &mut expected);
        expected.push('!');
        assert_eq!(source, expected);
        assert_eq!(
            crate::string_bytes::literal_bytes(&source),
            vec![b'e', 0xc3, 0xa9, b' ', 0xa9, b'!'],
            "the decode must round-trip to the original bytes"
        );
    }

    /// Verifies a source that spells a marker-range char itself cannot be mistaken for a decoded
    /// raw byte: it is re-encoded, so `literal_bytes` still returns its original UTF-8.
    #[test]
    fn a_private_use_source_char_round_trips_instead_of_colliding_with_a_byte_marker() {
        let original = "x\u{e0a9}y";
        let source = decode_physical_source(original.as_bytes().to_vec());
        assert_ne!(source, original, "the marker-range char must be re-encoded");
        assert_eq!(
            crate::string_bytes::literal_bytes(&source),
            original.as_bytes().to_vec()
        );
    }

    /// Verifies a nested file parse starts coercive and cannot leak its `strict_types` state
    /// back to the includer, which is what makes the directive per-file after include merging.
    #[test]
    fn nested_parse_scopes_strict_types_to_one_file() {
        with_parse_mode(SourceProfile::new(SourceMode::Php), || {
            assert!(!current_strict_types());
            declare_strict_types(true);
            assert!(current_strict_types());

            // An `include`d file parses inside the includer's scope and must start coercive.
            with_parse_mode(SourceProfile::new(SourceMode::Php), || {
                assert!(!current_strict_types());
                declare_strict_types(true);
            });
            assert!(current_strict_types());

            with_parse_mode(SourceProfile::new(SourceMode::Php), || {
                assert!(!current_strict_types());
            });
            assert!(current_strict_types());
        });
        assert!(!current_strict_types());
    }

    /// Verifies a statement-rewriting pass re-installing a statement's profile restores both the
    /// language mode and the `strict_types` flag, so a rebuilt node keeps its file's binding
    /// rules instead of silently reverting to coercive.
    #[test]
    fn reinstalling_a_profile_restores_both_fields() {
        let strict = SourceProfile {
            mode: SourceMode::Php,
            strict_types: true,
        };
        with_parse_mode(strict, || {
            assert_eq!(current_parse_mode(), SourceMode::Php);
            assert!(current_strict_types());
        });
        assert_eq!(current_parse_mode(), SourceMode::Internal);
        assert!(!current_strict_types());
    }

    /// Verifies strict PHP applies only to PHP-mode user source.
    #[test]
    fn strict_php_is_source_mode_aware() {
        assert!(SourceMode::Php.strict_php_is_effective(true));
        assert!(!SourceMode::Php.strict_php_is_effective(false));
        assert!(!SourceMode::Lfc.strict_php_is_effective(true));
        assert!(!SourceMode::Internal.strict_php_is_effective(true));
    }

    /// A file walked several times in a compile is opened once.
    #[test]
    fn a_source_is_opened_once_per_compile() {
        let dir = memo_test_dir("opened_once");
        let path = dir.join("a.php");
        std::fs::write(&path, "<?php echo 1;").unwrap();
        reset_source_memos();
        let before = SOURCE_OPENS.with(std::cell::Cell::get);
        for _ in 0..3 {
            assert_eq!(read_physical_source(&path).unwrap(), "<?php echo 1;");
        }
        assert_eq!(SOURCE_OPENS.with(std::cell::Cell::get) - before, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A rewritten file is read again, so a remembered version can never reach a compile.
    #[test]
    fn a_rewritten_source_is_read_again() {
        let dir = memo_test_dir("rewritten");
        let path = dir.join("a.php");
        std::fs::write(&path, "<?php echo 1;").unwrap();
        reset_source_memos();
        assert_eq!(read_physical_source(&path).unwrap(), "<?php echo 1;");
        std::fs::write(&path, "<?php echo 22;").unwrap();
        assert_eq!(read_physical_source(&path).unwrap(), "<?php echo 22;");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A path that does not exist yet is not remembered as missing.
    #[test]
    fn a_failed_canonicalization_is_not_remembered() {
        let dir = memo_test_dir("failed_canonical");
        let path = dir.join("later.php");
        reset_source_memos();
        assert!(canonical(&path).is_err());
        std::fs::write(&path, "<?php").unwrap();
        assert_eq!(canonical(&path).unwrap(), path.canonicalize().unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A fresh directory for one memo test.
    fn memo_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("elephc-source-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
