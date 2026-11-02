//! Purpose:
//! Layout of the module reflection table: the data a `ReflectionClass` is built from.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection::class_table`, which emits the rows.
//! - `crate::types::checker::builtin_types::reflection`, whose synthesized constructors read them
//!   through `__elephc_refl_row()`, `__elephc_refl_word()` and `__elephc_refl_pool_str()`.
//!
//! Key details:
//! - One row per reflected class-like, [`ROW_WORDS`] 8-byte words each. A word is a count, a flag
//!   set, a line, a code address, or the address of a string pool.
//! - A string pool is an array of `{pointer, length}` pairs; a scalar string is a pool of one, a
//!   list is a pool followed by its count in the next word of the row.
//! - The emitter and the constructors agree on this file and on nothing else, so adding a field
//!   is one constant here plus the code that reads it. Everything a reflector needs that is not
//!   plain data (an attribute argument evaluated at run time, a live static property) is reached
//!   through a code address in the row, never by growing per-class code.

/// 8-byte words per table slot (the stride `__elephc_refl_word()` multiplies a row index by).
pub const ROW_WORDS: usize = 16;

/// 8-byte words of a class row: it spans two consecutive slots, so a word index of 16 or more
/// reads the following slot, which is how the class words below are addressed.
pub const CLASS_ROW_WORDS: usize = 48;

/// Word indices of a row.
pub mod word {
    /// Pool of one: the canonical class-like name.
    pub const NAME: usize = 0;
    /// Flag bits, see [`super::flag`].
    pub const FLAGS: usize = 2;
    /// `ReflectionClass::getModifiers()`.
    pub const MODIFIERS: usize = 3;
    /// Pool of one: the declaring source file (valid when [`super::flag::HAS_FILE`] is set).
    pub const FILE: usize = 4;
    pub const START_LINE: usize = 6;
    pub const END_LINE: usize = 7;
    /// Pool of one: the doc comment (valid when [`super::flag::HAS_DOC`] is set).
    pub const DOC: usize = 8;
    /// Pool and count (next word): implemented interface names.
    pub const INTERFACES: usize = 10;
    pub const TRAITS: usize = 12;
    /// Ancestor class names, nearest first.
    pub const PARENTS: usize = 14;
    pub const METHODS: usize = 16;
    pub const PROPERTIES: usize = 18;
    pub const CONSTANTS: usize = 20;
    /// Code address filling the slots that are not plain data, for `ReflectionObject`
    /// (0: nothing to fill). `ReflectionObject` redeclares the class slots after its parent's,
    /// so it needs a filler of its own.
    pub const OBJECT_EXTRAS: usize = 22;
    /// Code address filling the slots that are not plain data, for `ReflectionClass`
    /// (0: nothing to fill).
    pub const EXTRAS: usize = 24;
    /// The same for `ReflectionEnum`.
    pub const ENUM_EXTRAS: usize = 25;
    /// Enum case names (pool and count), enums only.
    pub const CASES: usize = 26;
    /// Address of the value node of `getConstants()` (0: none or not expressible as data).
    pub const CONSTANT_VALUES: usize = 28;
    /// Address of the value node of `getDefaultProperties()` (0: likewise).
    pub const DEFAULT_VALUES: usize = 29;
    /// Address of one modifier word per constant, in the order of the constant names.
    pub const CONSTANT_MODIFIERS: usize = 30;
    /// Pool and count (next word): the methods `getMethods()` lists. `METHODS` is what
    /// `hasMethod()` answers for, which also holds an ancestor's private methods.
    pub const METHOD_MEMBERS: usize = 32;
    /// Pool and count (next word): the properties `getProperties()` lists.
    pub const PROPERTY_MEMBERS: usize = 34;
}

/// Bits of the [`word::FLAGS`] word.
pub mod flag {
    pub const FINAL: i64 = 1;
    pub const ABSTRACT: i64 = 2;
    pub const INTERFACE: i64 = 4;
    pub const TRAIT: i64 = 8;
    pub const ENUM: i64 = 16;
    pub const READONLY: i64 = 32;
    pub const ANONYMOUS: i64 = 64;
    pub const INSTANTIABLE: i64 = 128;
    pub const CLONEABLE: i64 = 256;
    pub const ITERABLE: i64 = 512;
    pub const INTERNAL: i64 = 1024;
    pub const USER_DEFINED: i64 = 2048;
    pub const HAS_DOC: i64 = 4096;
    pub const HAS_FILE: i64 = 8192;
    pub const BACKED: i64 = 16384;
    /// The row's reflector fills its member reflectors by constructing them from the row's lists
    /// (a closed-world build); otherwise the extras filler installs per-class member fillers.
    pub const GENERIC_MEMBERS: i64 = 32768;
}

/// Word indices of a member row (one slot): a constant, an enum case, a property or a method.
pub mod member {
    /// Pool of one: the member name as declared.
    pub const NAME: usize = 0;
    /// Pool of one: the declaring class name (`$member->class`).
    pub const CLASS: usize = 1;
    /// `getModifiers()`.
    pub const MODIFIERS: usize = 2;
    /// Bits of [`member_flag`].
    pub const FLAGS: usize = 3;
    /// Address of a type node (0: untyped), see [`super::type_node`].
    pub const TYPE: usize = 4;
    /// Address of the value node of a constant, an enum case or a property default (0: none).
    pub const VALUE: usize = 5;
    /// Reserved doc-comment pool; function rows reuse this slot for their source filename.
    pub const DOC: usize = 6;
    /// Code address filling the slots that are not plain data (0: nothing to fill).
    pub const EXTRAS: usize = 7;
    /// Pool of one: the `__toString()` text.
    pub const STRING: usize = 8;
    /// Address of the value node of an enum case's backing value (0: none).
    pub const BACKING_VALUE: usize = 9;
    /// Integer payload: a method's required parameter count, a parameter's position.
    pub const COUNT: usize = 10;
    /// Slot of the first parameter row of a method; its parameters follow it in the table (0: none).
    pub const PARAMETERS: usize = 11;
    /// Number of parameter rows of a method.
    pub const PARAMETER_COUNT: usize = 12;
    /// Pool of one: the class of a method's prototype (valid with [`super::member_flag::HAS_PROTOTYPE`]).
    pub const PROTOTYPE_CLASS: usize = 13;
    /// Pool of one: the name of a method's prototype.
    pub const PROTOTYPE_NAME: usize = 14;
    /// Pool of one: the name of the function declaring a parameter.
    pub const FUNCTION: usize = 9;
    /// A function row's first and last source line (valid with [`super::member_flag::HAS_FILE`]);
    /// its file is the [`DOC`] pool.
    pub const START_LINE: usize = 9;
    pub const END_LINE: usize = 15;
}

/// Bits of the [`member::FLAGS`] word.
pub mod member_flag {
    pub const PUBLIC: i64 = 1;
    pub const PROTECTED: i64 = 2;
    pub const PRIVATE: i64 = 4;
    pub const STATIC: i64 = 8;
    pub const FINAL: i64 = 16;
    pub const ABSTRACT: i64 = 32;
    pub const READONLY: i64 = 64;
    pub const PROMOTED: i64 = 128;
    pub const VIRTUAL: i64 = 256;
    pub const ENUM_CASE: i64 = 512;
    // Bit 10 is reserved; member rows do not currently encode doc-comment presence.
    pub const HAS_DEFAULT: i64 = 2048;
    pub const HAS_HOOKS: i64 = 4096;
    pub const DEPRECATED: i64 = 8192;
    pub const GENERATOR: i64 = 16384;
    pub const RETURNS_REFERENCE: i64 = 32768;
    pub const HAS_PROTOTYPE: i64 = 65536;
    /// Parameter rows.
    pub const OPTIONAL: i64 = 1 << 20;
    pub const VARIADIC: i64 = 1 << 21;
    pub const BY_REFERENCE: i64 = 1 << 22;
    pub const HAS_TYPE: i64 = 1 << 23;
    pub const ALLOWS_NULL: i64 = 1 << 24;
    pub const ARRAY_TYPE: i64 = 1 << 25;
    pub const CALLABLE_TYPE: i64 = 1 << 26;
    pub const DEFAULT_CONSTANT: i64 = 1 << 27;
    /// Function rows.
    pub const CLOSURE: i64 = 1 << 28;
    pub const HAS_FILE: i64 = 1 << 29;
}

/// Kind byte of a member key, see [`member_key`].
pub mod member_kind {
    pub const CONSTANT: u8 = b'c';
    pub const CASE: u8 = b'e';
    pub const PROPERTY: u8 = b'p';
    pub const METHOD: u8 = b'm';
    pub const FUNCTION: u8 = b'f';
}

/// Type nodes: four words `[tag, flags, a, b]`.
pub mod type_node {
    /// `flags` bit 0: allows null; bit 1: builtin. `a` is a pool of one: the type name.
    pub const NAMED: i64 = 1;
    /// `flags` bit 0: allows null. `a` is the address of `b` addresses of named type nodes.
    pub const UNION: i64 = 2;
    pub const INTERSECTION: i64 = 3;
}

/// The index key of a member: kind byte, NUL, lowercase class name, NUL, member name (lowercase
/// for a method). Class names cannot hold a NUL, so a member key never meets a class key.
pub fn member_key(kind: u8, class_lowercase: &str, name: &str) -> Vec<u8> {
    let mut key = vec![kind, 0];
    key.extend_from_slice(class_lowercase.as_bytes());
    key.push(0);
    key.extend_from_slice(name.as_bytes());
    key
}

/// Value nodes: three words `[tag, a, b]`, decoded by `ReflectionClass::__decode()`.
pub mod value {
    pub const NULL: i64 = 0;
    /// `a` is 0 or 1.
    pub const BOOL: i64 = 1;
    pub const INT: i64 = 2;
    /// `a` is a pool of one string holding the decimal text.
    pub const FLOAT: i64 = 3;
    /// `a` is a pool of one string.
    pub const STRING: i64 = 4;
    /// `a` is the address of `b` entries of six words each: a key node, then a value node.
    pub const ARRAY: i64 = 5;
    /// `a` is a pool of two strings: the enum name and the case name.
    pub const ENUM_CASE: i64 = 6;
    pub const INFINITY: i64 = 7;
    pub const NEGATIVE_INFINITY: i64 = 8;
    pub const NAN: i64 = 9;
    /// `a` is a pool of one: the class name; `b` is the address of an `ARRAY` value node holding
    /// the constructor arguments (a parameter default such as `new Foo(1)`).
    pub const OBJECT: i64 = 10;
    /// Bytes per entry of an [`ARRAY`] node.
    pub const ENTRY_BYTES: i64 = 48;
}

/// Label of the row array.
/// Whether the table-driven class reflectors are on: always in a closed-world build (it has no
/// interpreter to fall back on), and in a default build only with `ELEPHC_TABLE_REFLECTION=1`
/// until the per-class generated path is retired.
pub fn enabled() -> bool {
    enabled_for(crate::closed_world::is_enabled())
}

/// [`enabled`] for code that runs on a codegen worker thread, where the thread-local closed-world
/// switch is not set: pass `module.closed_world`.
pub fn enabled_for(closed_world: bool) -> bool {
    closed_world
        || std::env::var_os("ELEPHC_TABLE_REFLECTION").is_some_and(|value| value != "0")
}

/// The member-reflector slots of a class reflector that are built from the table's name lists
/// when first read (`__fill_generic<slot>`), instead of by code emitted per class. A closed-world
/// build builds all of them that way; a default build with the table reflectors on does so for
/// the slots whose member reflector is table-driven.
pub fn generic_slots() -> &'static [&'static str] {
    generic_slots_for(crate::closed_world::is_enabled())
}

/// [`generic_slots`] for code that runs on a codegen worker thread, where the thread-local
/// closed-world switch is not set: pass `module.closed_world`.
pub fn generic_slots_for(closed_world: bool) -> &'static [&'static str] {
    const ALL: &[&str] = &[
        "__reflection_constants",
        "__methods",
        "__constructor",
        "__parent_class",
        "__properties",
        "__cases",
    ];
    const READY: &[&str] = &[
        "__reflection_constants",
        "__cases",
        "__properties",
        "__methods",
        "__constructor",
        "__parent_class",
    ];
    if closed_world {
        ALL
    } else if enabled_for(false) {
        READY
    } else {
        &[]
    }
}

/// The member reflectors (lowercase class names) whose constructors read the table.
pub fn table_member_class(lowercase_name: &str) -> bool {
    matches!(
        lowercase_name,
        "reflectionclassconstant"
            | "reflectionenumunitcase"
            | "reflectionenumbackedcase"
            | "reflectionproperty"
            | "reflectionmethod"
    )
}

pub const ROWS_LABEL: &str = "_refl_rows";

/// Routine that finds a row by lowercase class-like name: C-style `(ptr, len)` in, row index plus
/// one (0: none) out.
pub const LOOKUP_SYMBOL: &str = "__rt_refl_row_lookup";
