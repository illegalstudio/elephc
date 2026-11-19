//! Purpose:
//! libzip-compatible error codes and their user-facing status strings.
//!
//! Called from:
//! - `crate::archive` for manual validation failures and closed-archive status replay.
//! - `crate::abi` for the numeric status entry points.
//!
//! Key details:
//! - Codes match libzip 1.11.4's `ZIP_ER_*` values and PHP's `ZipArchive::ER_*` constants.
//! - Live archives report libzip's own error through `zip_get_error`; this table covers
//!   errors the bridge rejects before calling libzip and archives closed after a failed
//!   `zip_close`.

/// One libzip error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // The full libzip code set stays available for future operations.
pub(crate) enum ZipError {
    /// No error.
    Ok,
    /// Multi-disk archives are unsupported.
    MultiDisk,
    /// Renaming the temporary file failed.
    Rename,
    /// Closing the archive failed.
    Close,
    /// Seek failed.
    Seek,
    /// Read failed.
    Read,
    /// Write failed.
    Write,
    /// CRC mismatch.
    Crc,
    /// Archive is already closed.
    ZipClosed,
    /// No such entry.
    NoEnt,
    /// Entry already exists.
    Exists,
    /// Can't open file.
    Open,
    /// Can't create temporary file.
    TempOpen,
    /// zlib error.
    Zlib,
    /// Allocation failure.
    Memory,
    /// Entry changed.
    Changed,
    /// Compression method unsupported.
    CompNotSupported,
    /// Premature end of file.
    Eof,
    /// Invalid argument.
    Invalid,
    /// Not a zip archive.
    NotZip,
    /// Internal error.
    Internal,
    /// Inconsistent archive.
    Inconsistent,
    /// Can't remove file.
    Remove,
    /// Entry deleted.
    Deleted,
    /// Encryption method unsupported.
    EncryptNotSupported,
    /// Read-only archive.
    ReadOnly,
    /// No password supplied.
    NoPassword,
    /// Wrong password supplied.
    WrongPassword,
    /// Operation not supported.
    NotSupported,
    /// Resource still in use.
    InUse,
    /// Tell failed.
    Tell,
    /// Compressed data invalid.
    CompressedData,
    /// Operation cancelled.
    Cancelled,
    /// Data length mismatch.
    DataLength,
    /// Operation not allowed.
    NotAllowed,
    /// Truncated zip file.
    Truncated,
}

impl ZipError {
    /// Maps a `ZipArchive::ER_*` code back to its enum value.
    pub(crate) fn from_code(code: i64) -> Self {
        match code {
            1 => Self::MultiDisk, 2 => Self::Rename, 3 => Self::Close, 4 => Self::Seek,
            5 => Self::Read, 6 => Self::Write, 7 => Self::Crc, 8 => Self::ZipClosed,
            9 => Self::NoEnt, 10 => Self::Exists, 11 => Self::Open, 12 => Self::TempOpen,
            13 => Self::Zlib, 14 => Self::Memory, 15 => Self::Changed,
            16 => Self::CompNotSupported, 17 => Self::Eof, 18 => Self::Invalid,
            19 => Self::NotZip, 20 => Self::Internal, 21 => Self::Inconsistent,
            22 => Self::Remove, 23 => Self::Deleted, 24 => Self::EncryptNotSupported,
            25 => Self::ReadOnly, 26 => Self::NoPassword, 27 => Self::WrongPassword,
            28 => Self::NotSupported, 29 => Self::InUse, 30 => Self::Tell,
            31 => Self::CompressedData, 32 => Self::Cancelled, 33 => Self::DataLength,
            34 => Self::NotAllowed, 35 => Self::Truncated, _ => Self::Ok,
        }
    }

    /// Returns the `ZipArchive::ER_*` numeric code.
    pub(crate) fn code(self) -> i64 {
        match self {
            Self::Ok => 0, Self::MultiDisk => 1, Self::Rename => 2, Self::Close => 3,
            Self::Seek => 4, Self::Read => 5, Self::Write => 6, Self::Crc => 7,
            Self::ZipClosed => 8, Self::NoEnt => 9, Self::Exists => 10, Self::Open => 11,
            Self::TempOpen => 12, Self::Zlib => 13, Self::Memory => 14, Self::Changed => 15,
            Self::CompNotSupported => 16, Self::Eof => 17, Self::Invalid => 18,
            Self::NotZip => 19, Self::Internal => 20, Self::Inconsistent => 21,
            Self::Remove => 22, Self::Deleted => 23, Self::EncryptNotSupported => 24,
            Self::ReadOnly => 25, Self::NoPassword => 26, Self::WrongPassword => 27,
            Self::NotSupported => 28, Self::InUse => 29, Self::Tell => 30,
            Self::CompressedData => 31, Self::Cancelled => 32, Self::DataLength => 33,
            Self::NotAllowed => 34, Self::Truncated => 35,
        }
    }

    /// Returns libzip's English status string for this code.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Ok => "No error",
            Self::MultiDisk => "Multi-disk zip archives not supported",
            Self::Rename => "Renaming temporary file failed",
            Self::Close => "Closing zip archive failed",
            Self::Seek => "Seek error",
            Self::Read => "Read error",
            Self::Write => "Write error",
            Self::Crc => "CRC error",
            Self::ZipClosed => "Containing zip archive was closed",
            Self::NoEnt => "No such file",
            Self::Exists => "File already exists",
            Self::Open => "Can't open file",
            Self::TempOpen => "Failure to create temporary file",
            Self::Zlib => "Zlib error",
            Self::Memory => "Malloc failure",
            Self::Changed => "Entry has been changed",
            Self::CompNotSupported => "Compression method not supported",
            Self::Eof => "Premature EOF",
            Self::Invalid => "Invalid argument",
            Self::NotZip => "Not a zip archive",
            Self::Internal => "Internal error",
            Self::Inconsistent => "Zip archive inconsistent",
            Self::Remove => "Can't remove file",
            Self::Deleted => "Entry has been deleted",
            Self::EncryptNotSupported => "Encryption method not supported",
            Self::ReadOnly => "Read-only archive",
            Self::NoPassword => "No password provided",
            Self::WrongPassword => "Wrong password provided",
            Self::NotSupported => "Operation not supported",
            Self::InUse => "Resource still in use",
            Self::Tell => "Tell error",
            Self::CompressedData => "Compressed data invalid",
            Self::Cancelled => "Operation cancelled",
            Self::DataLength => "Data length mismatch",
            Self::NotAllowed => "Operation not allowed",
            Self::Truncated => "Truncated ZIP file",
        }
    }
}
