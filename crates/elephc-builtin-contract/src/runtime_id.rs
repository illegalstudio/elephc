//! Purpose:
//! Defines the versioned typed identity for builtin implementations callable
//! through the generated boxed-cell runtime ABI.
//!
//! Called from:
//! - Compiler runtime-wrapper emission.
//! - Magician registry assembly and runtime dispatch.
//!
//! Key details:
//! - Values are stable ABI numbers, distinct from hashed catalog `BuiltinId`s.
//! - Only builtins with an equivalent boxed-cell runtime contract are mapped.

use crate::BuiltinId;

/// Builtin operation supported by `__elephc_runtime_builtin_call_v1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum RuntimeBuiltinId {
    /// PHP `boolval` with one argument.
    Boolval = 1,
    /// PHP `floatval` with one argument.
    Floatval = 2,
    /// PHP `intval` with the default base.
    Intval = 3,
    /// PHP `is_array`.
    IsArray = 4,
    /// PHP `is_null`.
    IsNull = 5,
    /// PHP `abs`.
    Abs = 6,
    /// PHP `ceil`.
    Ceil = 7,
    /// PHP `floor`.
    Floor = 8,
    /// PHP `sqrt`.
    Sqrt = 9,
    /// PHP `fdiv`.
    Fdiv = 10,
    /// PHP `fmod`.
    Fmod = 11,
    /// PHP `pow`.
    Pow = 12,
    /// PHP `round` with its supported optional precision.
    Round = 13,
    /// PHP byte-string `strrev`.
    Strrev = 14,
    /// PHP `array_key_exists` over boxed key and array cells.
    ArrayKeyExists = 15,
    /// PHP `ob_get_level`.
    ObGetLevel = 16,
    /// PHP `ob_get_length`.
    ObGetLength = 17,
    /// PHP `ob_clean`.
    ObClean = 18,
    /// PHP `ob_flush`.
    ObFlush = 19,
    /// PHP `ob_end_clean`.
    ObEndClean = 20,
    /// PHP `ob_end_flush`.
    ObEndFlush = 21,
    /// PHP `date_default_timezone_get` through the shared AOT request state.
    DateDefaultTimezoneGet = 22,
    /// PHP `date_default_timezone_set` after eval-side identifier validation.
    DateDefaultTimezoneSet = 23,
}

/// Status returned by `__elephc_runtime_builtin_call_v1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RuntimeBuiltinStatus {
    /// `result_out` owns one fresh boxed result cell.
    Success = 0,
    /// Arity, type, or runtime helper failure with no result ownership transfer.
    RuntimeFatal = 1,
    /// A throwable is pending in generated runtime state.
    PendingThrowable = 2,
    /// The ID or requested arity is not implemented by this ABI version.
    Unsupported = 3,
}

impl RuntimeBuiltinStatus {
    /// Decodes a raw C-ABI status, failing closed for unknown values.
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Success),
            1 => Some(Self::RuntimeFatal),
            2 => Some(Self::PendingThrowable),
            3 => Some(Self::Unsupported),
            _ => None,
        }
    }
}

impl RuntimeBuiltinId {
    /// Every version-one runtime builtin in stable ABI order.
    pub const ALL: [Self; 23] = [
        Self::Boolval,
        Self::Floatval,
        Self::Intval,
        Self::IsArray,
        Self::IsNull,
        Self::Abs,
        Self::Ceil,
        Self::Floor,
        Self::Sqrt,
        Self::Fdiv,
        Self::Fmod,
        Self::Pow,
        Self::Round,
        Self::Strrev,
        Self::ArrayKeyExists,
        Self::ObGetLevel,
        Self::ObGetLength,
        Self::ObClean,
        Self::ObFlush,
        Self::ObEndClean,
        Self::ObEndFlush,
        Self::DateDefaultTimezoneGet,
        Self::DateDefaultTimezoneSet,
    ];

    /// Returns the canonical shared-contract identity implemented by this ABI ID.
    pub const fn builtin_id(self) -> BuiltinId {
        let name = match self {
            Self::Boolval => "boolval",
            Self::Floatval => "floatval",
            Self::Intval => "intval",
            Self::IsArray => "is_array",
            Self::IsNull => "is_null",
            Self::Abs => "abs",
            Self::Ceil => "ceil",
            Self::Floor => "floor",
            Self::Sqrt => "sqrt",
            Self::Fdiv => "fdiv",
            Self::Fmod => "fmod",
            Self::Pow => "pow",
            Self::Round => "round",
            Self::Strrev => "strrev",
            Self::ArrayKeyExists => "array_key_exists",
            Self::ObGetLevel => "ob_get_level",
            Self::ObGetLength => "ob_get_length",
            Self::ObClean => "ob_clean",
            Self::ObFlush => "ob_flush",
            Self::ObEndClean => "ob_end_clean",
            Self::ObEndFlush => "ob_end_flush",
            Self::DateDefaultTimezoneGet => "date_default_timezone_get",
            Self::DateDefaultTimezoneSet => "date_default_timezone_set",
        };
        BuiltinId::from_canonical_name(name)
    }

    /// Returns the stable integer passed through the versioned C ABI.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Decodes one raw version-one ABI value, failing closed for unknown IDs.
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Boolval),
            2 => Some(Self::Floatval),
            3 => Some(Self::Intval),
            4 => Some(Self::IsArray),
            5 => Some(Self::IsNull),
            6 => Some(Self::Abs),
            7 => Some(Self::Ceil),
            8 => Some(Self::Floor),
            9 => Some(Self::Sqrt),
            10 => Some(Self::Fdiv),
            11 => Some(Self::Fmod),
            12 => Some(Self::Pow),
            13 => Some(Self::Round),
            14 => Some(Self::Strrev),
            15 => Some(Self::ArrayKeyExists),
            16 => Some(Self::ObGetLevel),
            17 => Some(Self::ObGetLength),
            18 => Some(Self::ObClean),
            19 => Some(Self::ObFlush),
            20 => Some(Self::ObEndClean),
            21 => Some(Self::ObEndFlush),
            22 => Some(Self::DateDefaultTimezoneGet),
            23 => Some(Self::DateDefaultTimezoneSet),
            _ => None,
        }
    }

    /// Returns whether version one implements this PHP argument count.
    pub const fn supports_arity(self, arg_count: usize) -> bool {
        match self {
            Self::Boolval
            | Self::Floatval
            | Self::Intval
            | Self::IsArray
            | Self::IsNull
            | Self::Abs
            | Self::Ceil
            | Self::Floor
            | Self::Sqrt
            | Self::Strrev
            | Self::DateDefaultTimezoneSet => arg_count == 1,
            Self::Fdiv | Self::Fmod | Self::Pow | Self::ArrayKeyExists => arg_count == 2,
            Self::Round => arg_count == 1 || arg_count == 2,
            Self::ObGetLevel
            | Self::ObGetLength
            | Self::ObClean
            | Self::ObFlush
            | Self::ObEndClean
            | Self::ObEndFlush
            | Self::DateDefaultTimezoneGet => arg_count == 0,
        }
    }
}

/// Maps a shared contract identity onto the boxed runtime ABI when supported.
pub fn runtime_builtin_id(id: BuiltinId) -> Option<RuntimeBuiltinId> {
    RuntimeBuiltinId::ALL
        .into_iter()
        .find(|runtime_id| id == runtime_id.builtin_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every published runtime ID round-trips through its raw ABI value.
    #[test]
    fn runtime_builtin_ids_round_trip() {
        for raw in 1..=23 {
            let id = RuntimeBuiltinId::from_u32(raw).expect("published runtime ID must decode");
            assert_eq!(id.as_u32(), raw);
            assert!(id.supports_arity(match id {
                RuntimeBuiltinId::Fdiv
                | RuntimeBuiltinId::Fmod
                | RuntimeBuiltinId::Pow
                | RuntimeBuiltinId::ArrayKeyExists => 2,
                RuntimeBuiltinId::ObGetLevel
                | RuntimeBuiltinId::ObGetLength
                | RuntimeBuiltinId::ObClean
                | RuntimeBuiltinId::ObFlush
                | RuntimeBuiltinId::ObEndClean
                | RuntimeBuiltinId::ObEndFlush
                | RuntimeBuiltinId::DateDefaultTimezoneGet => 0,
                _ => 1,
            }));
        }
        assert_eq!(RuntimeBuiltinId::from_u32(0), None);
        assert_eq!(RuntimeBuiltinId::from_u32(24), None);
        assert_eq!(
            RuntimeBuiltinStatus::from_i32(0),
            Some(RuntimeBuiltinStatus::Success)
        );
        assert_eq!(RuntimeBuiltinStatus::from_i32(4), None);
    }

    /// Verifies only explicitly compatible catalog identities enter runtime dispatch.
    #[test]
    fn catalog_mapping_is_explicit_and_fail_closed() {
        assert_eq!(
            runtime_builtin_id(BuiltinId::from_canonical_name("abs")),
            Some(RuntimeBuiltinId::Abs)
        );
        assert_eq!(
            runtime_builtin_id(BuiltinId::from_canonical_name("settype")),
            None
        );
    }
}
