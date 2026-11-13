//! Purpose:
//! Declares the independently selectable strict guarantees and their CLI metadata.
//!
//! Called from:
//! - CLI parsing, help generation, and semantic profile consumers.
//!
//! Key details:
//! - Feature identifiers are stable ABI bits; one catalog owns names and help.

/// Declares stable feature IDs and their CLI metadata together.
macro_rules! features {
    ($($variant:ident = $id:literal => $flag:literal, $help:literal;)*) => {
        /// One independently selectable semantic guarantee.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum StrictFeature {
            $(#[doc = $help] $variant = $id,)*
        }

        /// Stable feature inventory used by option parsing and umbrella expansion.
        pub const FEATURES: &[FeatureSpec] = &[
            $(FeatureSpec { feature: StrictFeature::$variant, flag: $flag, help: $help },)*
        ];
    };
}

features! {
    Locals = 0 => "strict-locals", "Keep local binding types stable";
    Coercions = 1 => "strict-coercions", "Require explicit value conversions";
    Overflow = 2 => "strict-overflow", "Check integer overflow without float promotion";
    Arithmetic = 3 => "strict-arithmetic", "Use predictable numeric operator signatures";
    Casts = 4 => "strict-casts", "Check explicit conversion domains and ranges";
    Bools = 5 => "strict-bools", "Require boolean conditions and logical operands";
    Comparisons = 6 => "strict-comparisons", "Require noncoercive comparisons";
    Nullability = 7 => "strict-nullability", "Require non-null values where needed";
    Mixed = 8 => "strict-mixed", "Narrow dynamic values before incompatible operations";
    Strings = 9 => "strict-strings", "Require strings for concatenation";
    ArrayTypes = 10 => "strict-array-types", "Keep array element contracts stable";
    ArrayKeys = 11 => "strict-array-keys", "Preserve exact integer and string keys";
    Initialization = 12 => "strict-initialization", "Require initialized reads";
    Indexing = 13 => "strict-indexing", "Check ordinary reads for missing elements";
    DynamicCode = 14 => "strict-dynamic-code", "Require statically verified source code";
    Opcache = 15 => "strict-opcache", "Throw when invalidating AOT-frozen code";
}

/// Features integrated across compiler and eval entry points in this development build.
/// Existing local and OPcache contracts retain their current scope while stronger rules land.
pub const INTEGRATED_FEATURE_BITS: u64 = StrictFeature::Locals.bit()
    | StrictFeature::DynamicCode.bit() | StrictFeature::Opcache.bit();

/// User-facing metadata for one strict feature.
#[derive(Clone, Copy, Debug)]
pub struct FeatureSpec {
    /// Stable semantic identity.
    pub feature: StrictFeature,
    /// Long option name without its leading hyphens.
    pub flag: &'static str,
    /// Help description independent of a particular compiler frontend.
    pub help: &'static str,
}

impl StrictFeature {
    /// Returns the ABI bit belonging to this guarantee.
    pub const fn bit(self) -> u64 { 1u64 << self as u8 }

    /// Looks up an exact long option name without leading hyphens.
    pub fn from_flag(flag: &str) -> Option<Self> {
        FEATURES.iter().find(|spec| spec.flag == flag).map(|spec| spec.feature)
    }

    /// Returns this guarantee's catalogued CLI name without leading hyphens.
    pub fn flag(self) -> &'static str {
        FEATURES.iter().find(|spec| spec.feature == self).expect("catalogued feature").flag
    }
}
