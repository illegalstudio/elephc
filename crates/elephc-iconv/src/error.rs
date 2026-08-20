//! Purpose:
//! Defines the PHP-visible failure modes of the iconv extension and renders the
//! exact diagnostic line php-src emits for each one.
//!
//! Called from:
//! - Every operation module in this crate when a conversion cannot complete.
//! - `crate::abi` while packing a diagnostic into a result block for the AOT runtime.
//!
//! Key details:
//! - Severities mirror php-src: charset problems are warnings, byte-level problems are notices.
//! - `_php_iconv_mime_decode` reports its source charset as the literal `???` placeholder,
//!   so the MIME decoders pass that spelling through instead of the parsed charset name.

/// One PHP diagnostic severity emitted by the iconv extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    /// php-src `E_WARNING`.
    Warning,
    /// php-src `E_NOTICE`.
    Notice,
}

impl Severity {
    /// Returns the label elephc's runtime prints in front of the message body.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Warning => "Warning",
            Severity::Notice => "Notice",
        }
    }
}

/// One failure of an iconv operation, carrying everything its diagnostic needs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IconvError {
    /// `iconv_open()` refused the charset pair.
    WrongCharset {
        /// Source charset as spelled in the diagnostic.
        from: String,
        /// Destination charset as spelled in the diagnostic.
        to: String,
    },
    /// The input ended in the middle of a valid multibyte sequence.
    IncompleteChar,
    /// The input contained a byte sequence the source charset does not allow.
    IllegalSequence,
    /// A MIME encoded-word could not be parsed.
    MalformedString,
    /// The requested field would not fit inside the configured line length.
    TooBig,
}

impl IconvError {
    /// Returns the severity php-src attaches to this failure.
    pub fn severity(&self) -> Severity {
        match self {
            IconvError::IncompleteChar | IconvError::IllegalSequence => Severity::Notice,
            _ => Severity::Warning,
        }
    }

    /// Renders the message body without the severity label or the calling function name.
    pub fn message(&self) -> String {
        match self {
            IconvError::WrongCharset { from, to } => format!(
                "Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"
            ),
            IconvError::IncompleteChar => {
                "Detected an incomplete multibyte character in input string".to_string()
            }
            IconvError::IllegalSequence => {
                "Detected an illegal character in input string".to_string()
            }
            IconvError::MalformedString => "Malformed string".to_string(),
            IconvError::TooBig => "Buffer length exceeded".to_string(),
        }
    }

    /// Renders the complete `iconv_xxx(): ...` diagnostic body for one PHP function.
    pub fn php_message(&self, function: &str) -> String {
        format!("{function}(): {}", self.message())
    }

    /// Renders the complete diagnostic line elephc's AOT runtime writes to stderr.
    pub fn diagnostic_line(&self, function: &str) -> String {
        format!(
            "{}: {}\n",
            self.severity().label(),
            self.php_message(function)
        )
    }
}

/// Result alias used by every operation in this crate.
pub type IconvResult<T> = Result<T, IconvError>;

impl IconvError {
    /// Rewrites a charset failure so it names the charsets php-src reports.
    ///
    /// php-src renders one diagnostic per PHP function regardless of which internal
    /// `iconv_open()` failed, so the caller supplies the pair its message must show.
    pub fn with_reported_charsets(self, from: &str, to: &str) -> Self {
        match self {
            IconvError::WrongCharset { .. } => IconvError::WrongCharset {
                from: from.to_string(),
                to: to.to_string(),
            },
            other => other,
        }
    }
}
