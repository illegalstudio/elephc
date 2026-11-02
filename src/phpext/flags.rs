//! Purpose:
//! Parses compiler flag words returned by php-config without running a shell.
//!
//! Called from:
//! - `super::sdk::PhpSdk::discover()` for SDK include flags and the C compiler command.
//!
//! Key details:
//! - Quotes and backslash escaping group arguments; substitutions are never evaluated.

/// Splits shell-style words while preserving quoted empty arguments and literal substitutions.
pub(super) fn split_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' && quote != Some('\'') {
            let next = chars.next().ok_or_else(|| "unterminated escape in PHP SDK compiler flags".to_string())?;
            if quote == Some('"') && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                word.push('\\');
            }
            if next != '\n' {
                word.push(next);
                started = true;
            }
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_some() {
            word.push(ch);
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            started = true;
        } else if ch.is_whitespace() {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(ch);
            started = true;
        }
    }
    if quote.is_some() {
        return Err("unterminated quote or escape in PHP SDK compiler flags".to_string());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps quoted include directories and escaped spaces in single compiler arguments.
    #[test]
    fn groups_sdk_paths_without_shell_evaluation() {
        assert_eq!(split_words("-I'/SDK path/php' -I/other\\ path -DNAME=\"a b\"").unwrap(),
            ["-I/SDK path/php", "-I/other path", "-DNAME=a b"]);
        assert_eq!(split_words("'$(touch nope)' '`uname`' \"\"").unwrap(),
            ["$(touch nope)", "`uname`", ""]);
        assert_eq!(split_words("-I\"C:\\unusual\\path\" before\\\nafter").unwrap(),
            ["-IC:\\unusual\\path", "beforeafter"]);
    }

    /// Rejects malformed quoting instead of passing truncated flags to the compiler.
    #[test]
    fn rejects_incomplete_flags() {
        for flags in ["-I'open", "-I\"open", "path\\"] {
            assert!(split_words(flags).is_err());
        }
    }
}
