//! Purpose:
//! The finished user assembly as the code generator hands it over: an optional prefix the
//! emitter already spilled to disk, the text still in memory, and the late rewrites (relaxed
//! branches, the visibility footer) kept as a patch list instead of applied by copying.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()`, which builds it.
//! - `crate::pipeline::backend`, which writes it to the `.s` file.
//!
//! Key details:
//! - On the Symfony `--web` build the text is 1.5 GB. Held in memory, then copied once to relax
//!   branches and again to append the visibility footer, it set most of the 9.5 GB peak
//!   footprint. Now the bulk stays on disk, the patches are small, and `write_to` streams.
//! - Edit offsets are positions in the whole text: spilled prefix followed by `text`.
//! - `into_string` produces byte-for-byte what the old copying passes produced; tests and the
//!   rare paths that need a `&str` (debug line injection, source maps) use it.

use crate::codegen_support::emit::SpilledText;
use crate::mapped_file::MappedFile;
use std::io::Write;

/// One replacement of `whole[start..end]`, with ranges ascending and non-overlapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) replacement: String,
}

/// Finished user assembly: `prefix` + `text` with `edits` applied, followed by `suffix`.
#[derive(Debug, Default)]
pub struct UserAssembly {
    prefix: Option<SpilledText>,
    text: String,
    edits: Vec<TextEdit>,
    suffix: String,
}

/// A read-only view of the spilled prefix (empty when there is none).
pub(crate) struct PrefixView {
    mapped: Option<MappedFile>,
}

impl PrefixView {
    /// Maps `prefix`, or yields an empty view.
    pub(crate) fn open(prefix: Option<&SpilledText>) -> std::io::Result<Self> {
        let Some(prefix) = prefix else {
            return Ok(Self { mapped: None });
        };
        let mapped = MappedFile::open(&prefix.path).ok_or_else(|| {
            std::io::Error::other(format!("cannot map spilled assembly {}", prefix.path.display()))
        })?;
        Ok(Self {
            mapped: Some(mapped),
        })
    }

    /// The prefix text.
    pub(crate) fn as_str(&self) -> &str {
        match &self.mapped {
            // The emitter wrote this file from `String`s, so it is valid UTF-8.
            Some(mapped) => std::str::from_utf8(mapped.bytes()).unwrap_or(""),
            None => "",
        }
    }
}

impl UserAssembly {
    /// Wraps text that needs no late rewrites.
    pub fn from_text(text: String) -> Self {
        Self {
            prefix: None,
            text,
            edits: Vec::new(),
            suffix: String::new(),
        }
    }

    /// Builds a patched assembly. `edits` must be ascending and non-overlapping.
    pub(crate) fn new(
        prefix: Option<SpilledText>,
        text: String,
        edits: Vec<TextEdit>,
        suffix: String,
    ) -> Self {
        debug_assert!(edits.windows(2).all(|pair| pair[0].end <= pair[1].start));
        Self {
            prefix,
            text,
            edits,
            suffix,
        }
    }

    /// Length of the spilled prefix.
    fn prefix_len(&self) -> usize {
        self.prefix.as_ref().map_or(0, |prefix| prefix.len)
    }

    /// Visits the final bytes after `from` (a position in prefix + text) in order.
    fn for_each_piece_from<E>(
        &self,
        prefix: &str,
        from: usize,
        mut visit: impl FnMut(&str) -> Result<(), E>,
    ) -> Result<(), E> {
        let split = prefix.len();
        // Emits whole[lo..hi], which may straddle the prefix/text boundary.
        let range = |lo: usize, hi: usize, visit: &mut dyn FnMut(&str) -> Result<(), E>| {
            if lo < split {
                visit(&prefix[lo..hi.min(split)])?;
            }
            if hi > split {
                visit(&self.text[lo.max(split) - split..hi - split])?;
            }
            Ok(())
        };
        let total = split + self.text.len();
        let mut cursor = from;
        for edit in self.edits.iter().filter(|edit| edit.start >= from) {
            range(cursor, edit.start, &mut visit)?;
            visit(&edit.replacement)?;
            cursor = edit.end;
        }
        range(cursor, total, &mut visit)?;
        visit(&self.suffix)
    }

    /// Writes the final assembly to `path`, consuming the spilled prefix file.
    ///
    /// When no edit touches the prefix, the prefix file itself becomes the output: the rest is
    /// appended to it and it is renamed, so the bulk of the text is never copied.
    pub fn write_to(self, path: &std::path::Path) -> std::io::Result<()> {
        let prefix_len = self.prefix_len();
        let untouched_prefix = self.edits.iter().all(|edit| edit.start >= prefix_len);
        if let (Some(prefix), true) = (self.prefix.as_ref(), untouched_prefix) {
            let file = std::fs::OpenOptions::new().append(true).open(&prefix.path)?;
            let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
            self.append_after_prefix(prefix_len, &mut out)?;
            out.flush()?;
            drop(out);
            std::fs::rename(&prefix.path, path)?;
            return Ok(());
        }
        let view = PrefixView::open(self.prefix.as_ref())?;
        let file = std::fs::File::create(path)?;
        let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
        self.for_each_piece_from(view.as_str(), 0, |piece| out.write_all(piece.as_bytes()))?;
        out.flush()?;
        drop(view);
        if let Some(prefix) = &self.prefix {
            let _ = std::fs::remove_file(&prefix.path);
        }
        Ok(())
    }

    /// Writes everything after the prefix, when no edit touches the prefix.
    fn append_after_prefix(&self, prefix_len: usize, out: &mut impl Write) -> std::io::Result<()> {
        let mut cursor = 0usize;
        for edit in &self.edits {
            let start = edit.start - prefix_len;
            out.write_all(self.text[cursor..start].as_bytes())?;
            out.write_all(edit.replacement.as_bytes())?;
            cursor = edit.end - prefix_len;
        }
        out.write_all(self.text[cursor..].as_bytes())?;
        out.write_all(self.suffix.as_bytes())
    }

    /// Materializes the final assembly as one string.
    pub fn into_string(self) -> String {
        let view = PrefixView::open(self.prefix.as_ref()).expect("spilled assembly is readable");
        let prefix = view.as_str();
        let growth: usize = self
            .edits
            .iter()
            .map(|edit| edit.replacement.len())
            .sum::<usize>()
            + self.suffix.len();
        let mut out = String::with_capacity(prefix.len() + self.text.len() + growth);
        let _ = self.for_each_piece_from(prefix, 0, |piece| {
            out.push_str(piece);
            Ok::<(), std::convert::Infallible>(())
        });
        drop(view);
        if let Some(prefix) = &self.prefix {
            let _ = std::fs::remove_file(&prefix.path);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("elephc-user-asm-{}-{name}", std::process::id()))
    }

    #[test]
    fn edits_and_suffix_apply_in_order() {
        let assembly = UserAssembly::new(
            None,
            "aaa\nbbb\nccc\n".to_string(),
            vec![
                TextEdit { start: 0, end: 4, replacement: "A\n".to_string() },
                TextEdit { start: 8, end: 12, replacement: "C1\nC2\n".to_string() },
            ],
            "tail\n".to_string(),
        );
        assert_eq!(assembly.into_string(), "A\nbbb\nC1\nC2\ntail\n");
    }

    /// A spilled prefix and the in-memory rest produce the same bytes as one text, whether an
    /// edit lands in the prefix (copying path) or only after it (append-and-rename path).
    #[test]
    fn spilled_prefix_matches_the_single_text_form() {
        for (label, edit_start) in [("in-prefix", 2usize), ("after-prefix", 6usize)] {
            let prefix_path = temp(&format!("{label}.spill"));
            std::fs::write(&prefix_path, "p1\np2\n").unwrap();
            let edit = TextEdit {
                start: edit_start,
                end: edit_start + 3,
                replacement: "EDIT\n".to_string(),
            };
            let spilled = || {
                UserAssembly::new(
                    Some(SpilledText { path: prefix_path.clone(), len: 6 }),
                    "t1\nt2\n".to_string(),
                    vec![edit.clone()],
                    "sfx\n".to_string(),
                )
            };
            let expected = UserAssembly::new(
                None,
                "p1\np2\nt1\nt2\n".to_string(),
                vec![edit.clone()],
                "sfx\n".to_string(),
            )
            .into_string();
            let out = temp(&format!("{label}.s"));
            spilled().write_to(&out).unwrap();
            let written = std::fs::read_to_string(&out).unwrap();
            let _ = std::fs::remove_file(&out);
            assert_eq!(written, expected, "{label}");
            assert!(!prefix_path.exists(), "{label}: the prefix file is consumed");
        }
    }
}
