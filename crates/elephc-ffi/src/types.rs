//! Purpose:
//! Owns the FFI C type arena, canonical PHP-visible type names, and scalar/layout
//! predicates shared by the declaration parser, the ABI layer, and the CData engine.
//!
//! Called from:
//! - `crate::parser`, `crate::context`, `crate::memory`, and `crate::call`.
//!
//! Key details:
//! - Types are arena indices so recursive structs can point at themselves through a
//!   pointer without cloning or reference cycles.
//! - Display names follow php-src's `ext/ffi` spellings (`int` is `int32_t`, anonymous
//!   tags are `<anonymous>`), because `var_dump()` and `FFI\CType::getName()` expose them.
//! - Sizes and alignments are C `sizeof`/`alignof` values for the LP64 targets elephc
//!   supports; target-specific `long double` is selected by `cfg`.

use std::collections::HashMap;

/// Stable arena index for one C type.
pub type TypeId = usize;

// Attribute bits reported by `FFI\CType::getAttributes()`.
pub const ATTR_CONST: u32 = 1;
pub const ATTR_INCOMPLETE_TAG: u32 = 2;
pub const ATTR_VARIADIC: u32 = 4;
pub const ATTR_INCOMPLETE_ARRAY: u32 = 8;
pub const ATTR_VLA: u32 = 16;
pub const ATTR_UNION: u32 = 32;
pub const ATTR_PACKED: u32 = 64;
pub const ATTR_MS_STRUCT: u32 = 128;
pub const ATTR_GCC_STRUCT: u32 = 256;

// `FFI\CType` kind constants.
pub const TYPE_VOID: u32 = 0;
pub const TYPE_FLOAT: u32 = 1;
pub const TYPE_DOUBLE: u32 = 2;
pub const TYPE_LONGDOUBLE: u32 = 3;
pub const TYPE_UINT8: u32 = 4;
pub const TYPE_SINT8: u32 = 5;
pub const TYPE_UINT16: u32 = 6;
pub const TYPE_SINT16: u32 = 7;
pub const TYPE_UINT32: u32 = 8;
pub const TYPE_SINT32: u32 = 9;
pub const TYPE_UINT64: u32 = 10;
pub const TYPE_SINT64: u32 = 11;
pub const TYPE_ENUM: u32 = 12;
pub const TYPE_BOOL: u32 = 13;
pub const TYPE_CHAR: u32 = 14;
pub const TYPE_POINTER: u32 = 15;
pub const TYPE_FUNC: u32 = 16;
pub const TYPE_ARRAY: u32 = 17;
pub const TYPE_STRUCT: u32 = 18;

/// One parsed C type.
#[derive(Clone, Debug)]
pub struct CType {
    /// Structural shape of the type.
    pub kind: TypeKind,
    /// C `sizeof` value.
    pub size: usize,
    /// C `alignof` value.
    pub align: usize,
    /// php-src attribute bits for this type.
    pub attrs: u32,
    /// Canonical PHP-visible spelling, e.g. `int32_t`, `struct _a`, `int32_t[2]`.
    pub display: String,
    /// Whether this occurrence is `const`-qualified.
    pub is_const: bool,
}

/// Structural C type shape.
#[derive(Clone, Debug)]
pub enum TypeKind {
    /// `void`.
    Void,
    /// C `_Bool`.
    Bool,
    /// C `char`, which PHP reads and writes as a one-byte string.
    Char { signed: bool },
    /// Integer of `bits` width; `signed` selects sign extension.
    Int { bits: u8, signed: bool },
    /// Floating-point of `bits` width.
    Float { bits: u8 },
    /// Enum backed by one integer type.
    Enum {
        underlying: TypeId,
        values: Vec<(String, i64)>,
        incomplete: bool,
    },
    /// Pointer to another type.
    Pointer(TypeId),
    /// Array of `len` elements; `None` is an incomplete/flexible array.
    Array { element: TypeId, len: Option<usize> },
    /// C function type.
    Func(FuncType),
    /// Struct or union.
    Struct(StructType),
    /// A tag declared but not yet defined.
    IncompleteTag { keyword: TagKeyword, tag: String },
}

/// Struct/union tag keyword.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagKeyword {
    Struct,
    Union,
    Enum,
}

impl TagKeyword {
    /// Returns the C keyword spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Union => "union",
            Self::Enum => "enum",
        }
    }
}

/// One field of a struct or union.
#[derive(Clone, Debug)]
pub struct Field {
    /// Field name; anonymous promoted records carry `None`.
    pub name: Option<String>,
    /// Field type.
    pub ty: TypeId,
    /// Byte offset of the field (or of its storage unit for a bit-field).
    pub offset: usize,
    /// Bit offset within the storage unit, or absolute bit offset for packed bit-fields.
    pub bit_offset: u8,
    /// Bit-field width.
    pub bit_width: Option<u8>,
    /// Whether the field itself is declared `const`.
    pub readonly: bool,
}

/// Parsed struct or union.
#[derive(Clone, Debug)]
pub struct StructType {
    /// Fields in declaration order.
    pub fields: Vec<Field>,
    /// True for a union.
    pub union: bool,
    /// True when the record carries `__attribute__((packed))`.
    pub packed: bool,
    /// Tag name, when the declaration was tagged.
    pub tag: Option<String>,
    /// True until the body has been parsed.
    pub incomplete: bool,
}

/// Parsed C function type.
#[derive(Clone, Debug)]
pub struct FuncType {
    /// Return type.
    pub ret: TypeId,
    /// Fixed parameter types.
    pub params: Vec<TypeId>,
    /// Whether the declaration ends in `...`.
    pub variadic: bool,
    /// php-src ABI selector (`FFI\CType::ABI_*`).
    pub abi: u32,
}

impl CType {
    /// Returns the `FFI\CType::TYPE_*` kind code.
    pub fn kind_code(&self) -> u32 {
        match &self.kind {
            TypeKind::Void => TYPE_VOID,
            TypeKind::Bool => TYPE_BOOL,
            TypeKind::Char { .. } => TYPE_CHAR,
            TypeKind::Int { bits: 8, signed: true } => TYPE_SINT8,
            TypeKind::Int { bits: 8, signed: false } => TYPE_UINT8,
            TypeKind::Int { bits: 16, signed: true } => TYPE_SINT16,
            TypeKind::Int { bits: 16, signed: false } => TYPE_UINT16,
            TypeKind::Int { bits: 32, signed: true } => TYPE_SINT32,
            TypeKind::Int { bits: 32, signed: false } => TYPE_UINT32,
            TypeKind::Int { bits: 64, signed: true } => TYPE_SINT64,
            TypeKind::Int { signed: false, .. } => TYPE_UINT64,
            TypeKind::Int { .. } => TYPE_SINT64,
            TypeKind::Float { bits: 32 } => TYPE_FLOAT,
            TypeKind::Float { .. } => TYPE_DOUBLE,
            TypeKind::Enum { .. } => TYPE_ENUM,
            TypeKind::Pointer(_) => TYPE_POINTER,
            TypeKind::Array { .. } => TYPE_ARRAY,
            TypeKind::Func(_) => TYPE_FUNC,
            TypeKind::Struct(_) => TYPE_STRUCT,
            TypeKind::IncompleteTag { keyword: TagKeyword::Enum, .. } => TYPE_ENUM,
            TypeKind::IncompleteTag { .. } => TYPE_STRUCT,
        }
    }

    /// Returns true for a scalar numeric type PHP exposes as `int` or `float`.
    pub fn is_numeric(&self) -> bool {
        matches!(self.kind, TypeKind::Int { .. } | TypeKind::Enum { .. } | TypeKind::Float { .. } | TypeKind::Bool)
    }

    /// Returns true for an integer-like type (including enums and `bool`).
    pub fn is_integer_like(&self) -> bool {
        matches!(self.kind, TypeKind::Int { .. } | TypeKind::Enum { .. } | TypeKind::Bool)
    }

    /// Returns true for a pointer type.
    pub fn is_pointer(&self) -> bool {
        matches!(self.kind, TypeKind::Pointer(_))
    }

    /// Returns true when the type cannot be instantiated because its shape is unknown.
    pub fn is_incomplete(&self) -> bool {
        match &self.kind {
            TypeKind::IncompleteTag { .. } => true,
            TypeKind::Struct(record) => record.incomplete,
            TypeKind::Enum { incomplete, .. } => *incomplete,
            TypeKind::Array { len, .. } => len.is_none(),
            _ => false,
        }
    }

    /// Returns true when the type is an array.
    pub fn is_array(&self) -> bool {
        matches!(self.kind, TypeKind::Array { .. })
    }
}

/// Arena of every C type known to the process.
#[derive(Default)]
pub struct TypeStore {
    types: Vec<CType>,
}

impl TypeStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    /// Inserts one type and returns its arena id.
    pub fn push(&mut self, ty: CType) -> TypeId {
        let id = self.types.len();
        self.types.push(ty);
        id
    }

    /// Returns a type by id.
    pub fn get(&self, id: TypeId) -> &CType {
        &self.types[id]
    }

    /// Returns a mutable type by id.
    pub fn get_mut(&mut self, id: TypeId) -> &mut CType {
        &mut self.types[id]
    }

    /// Replaces an already-allocated type.
    pub fn replace(&mut self, id: TypeId, ty: CType) {
        self.types[id] = ty;
    }

    /// Returns the number of allocated types.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Returns every allocated type, for tests and reflection.
    pub fn types(&self) -> &[CType] {
        &self.types
    }

    /// Returns whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Inserts a plain integer type.
    pub fn int(&mut self, bits: u8, signed: bool, display: &str) -> TypeId {
        self.push(CType {
            kind: TypeKind::Int { bits, signed },
            size: usize::from(bits / 8),
            align: usize::from(bits / 8),
            attrs: 0,
            display: display.to_string(),
            is_const: false,
        })
    }

    /// Inserts a plain floating-point type.
    pub fn float(&mut self, bits: u8, display: &str, size: usize, align: usize) -> TypeId {
        self.push(CType {
            kind: TypeKind::Float { bits },
            size,
            align,
            attrs: 0,
            display: display.to_string(),
            is_const: false,
        })
    }

    /// Inserts a pointer to `to`.
    pub fn pointer(&mut self, to: TypeId) -> TypeId {
        let display = match self.get(to).kind.clone() {
            TypeKind::Func(func) => self.function_pointer_display(&func),
            _ => format!("{}*", self.get(to).display),
        };
        self.push(CType {
            kind: TypeKind::Pointer(to),
            size: pointer_width(),
            align: pointer_width(),
            attrs: 0,
            display,
            is_const: false,
        })
    }

    /// Inserts an array of `len` elements; `None` is an incomplete array.
    pub fn array(&mut self, element: TypeId, len: Option<usize>) -> TypeId {
        let (size, align, attrs, display) = {
            let inner = self.get(element);
            let size = len.map_or(0, |len| inner.size.saturating_mul(len));
            let dimension = match len {
                Some(len) => format!("[{}]", len),
                None => "[]".to_string(),
            };
            // C prints the OUTERMOST dimension first, while the tree stores it as the
            // array whose element is the inner array. Insert the new dimension before
            // the first existing `[` so `int[2][3]` stays `int32_t[2][3]`.
            let display = match inner.display.find('[') {
                Some(index) => {
                    let mut text = inner.display.clone();
                    text.insert_str(index, &dimension);
                    text
                }
                None => format!("{}{}", inner.display, dimension),
            };
            let attrs = if len.is_none() { ATTR_INCOMPLETE_ARRAY } else { 0 };
            (size, inner.align.max(1), attrs, display)
        };
        self.push(CType {
            kind: TypeKind::Array { element, len },
            size,
            align,
            attrs,
            display,
            is_const: false,
        })
    }

    /// Inserts a function type.
    pub fn function(&mut self, func: FuncType) -> TypeId {
        let display = self.function_display(&func);
        let attrs = if func.variadic { ATTR_VARIADIC } else { 0 };
        self.push(CType {
            kind: TypeKind::Func(func),
            size: 0,
            align: 1,
            attrs,
            display,
            is_const: false,
        })
    }

    /// Renders a function type the way php-src's `var_dump` displays it.
    ///
    /// php-src's internal CData display omits the parameter list and prints `ret()`;
    /// `FFI\CType::getName()` reflection remains available for the full signature.
    pub fn function_display(&self, func: &FuncType) -> String {
        let ret = &self.get(func.ret).display;
        format!("{ret}()")
    }

    /// Renders the display name assigned to a function pointer type.
    pub fn function_pointer_display(&self, func: &FuncType) -> String {
        let ret = &self.get(func.ret).display;
        format!("{ret}(*)()")
    }

    /// Inserts the builtin types and their common typdef spellings.
    pub fn with_builtins() -> (Self, HashMap<String, TypeId>) {
        let mut store = Self::new();
        let mut names = HashMap::new();
        let add = |names: &mut HashMap<String, TypeId>, name: &str, id: TypeId| {
            names.insert(name.to_string(), id);
        };

        let void = store.push(CType {
            kind: TypeKind::Void,
            size: 0,
            align: 1,
            attrs: 0,
            display: "void".to_string(),
            is_const: false,
        });
        add(&mut names, "void", void);

        let bool_ty = store.push(CType {
            kind: TypeKind::Bool,
            size: 1,
            align: 1,
            attrs: 0,
            display: "bool".to_string(),
            is_const: false,
        });
        add(&mut names, "_Bool", bool_ty);
        add(&mut names, "bool", bool_ty);

        let char_ty = store.push(CType {
            kind: TypeKind::Char { signed: cfg!(any(target_os = "macos", target_os = "ios", target_arch = "x86_64")) },
            size: 1,
            align: 1,
            attrs: 0,
            display: "char".to_string(),
            is_const: false,
        });
        add(&mut names, "char", char_ty);

        let schar = store.int(8, true, "int8_t");
        let uchar = store.int(8, false, "uint8_t");
        let sshort = store.int(16, true, "int16_t");
        let ushort = store.int(16, false, "uint16_t");
        let sint = store.int(32, true, "int32_t");
        let uint = store.int(32, false, "uint32_t");
        let slong = store.int(64, true, "int64_t");
        let ulong = store.int(64, false, "uint64_t");
        let sllong = store.int(64, true, "int64_t");
        let ullong = store.int(64, false, "uint64_t");
        let float_ty = store.float(32, "float", 4, 4);
        let double_ty = store.float(64, "double", 8, 8);
        let (long_double_size, long_double_align) = if cfg!(any(target_os = "macos", target_os = "ios")) {
            (8, 8)
        } else {
            (16, 16)
        };
        let long_double = store.float(80, "long double", long_double_size, long_double_align);

        for (name, id) in [
            ("signed char", schar),
            ("int8_t", schar),
            ("int8", schar),
            ("unsigned char", uchar),
            ("uint8_t", uchar),
            ("uint8", uchar),
            ("short", sshort),
            ("short int", sshort),
            ("signed short", sshort),
            ("signed short int", sshort),
            ("int16_t", sshort),
            ("int16", sshort),
            ("unsigned short", ushort),
            ("unsigned short int", ushort),
            ("uint16_t", ushort),
            ("uint16", ushort),
            ("int", sint),
            ("signed", sint),
            ("signed int", sint),
            ("int32_t", sint),
            ("int32", sint),
            ("unsigned", uint),
            ("unsigned int", uint),
            ("uint32_t", uint),
            ("uint32", uint),
            ("long", slong),
            ("long int", slong),
            ("signed long", slong),
            ("signed long int", slong),
            ("int64_t", slong),
            ("int64", slong),
            ("long long", sllong),
            ("long long int", sllong),
            ("signed long long", sllong),
            ("signed long long int", sllong),
            ("unsigned long", ulong),
            ("unsigned long int", ulong),
            ("uint64_t", ulong),
            ("uint64", ulong),
            ("unsigned long long", ullong),
            ("unsigned long long int", ullong),
            ("float", float_ty),
            ("double", double_ty),
            ("long double", long_double),
        ] {
            add(&mut names, name, id);
        }

        let size_t = store.int(64, false, "uint64_t");
        let ssize_t = store.int(64, true, "int64_t");
        let ptrdiff_t = store.int(64, true, "int64_t");
        let intptr_t = store.int(64, true, "int64_t");
        let uintptr_t = store.int(64, false, "uint64_t");
        let intmax_t = store.int(64, true, "int64_t");
        let uintmax_t = store.int(64, false, "uint64_t");
        let off_t = store.int(64, true, "int64_t");
        let time_t = store.int(64, true, "int64_t");
        let wchar_t = store.int(32, true, "int32_t");
        let va_list = store.pointer(char_ty);
        add(&mut names, "__builtin_va_list", va_list);
        for (name, id) in [
            ("size_t", size_t),
            ("ssize_t", ssize_t),
            ("ptrdiff_t", ptrdiff_t),
            ("intptr_t", intptr_t),
            ("uintptr_t", uintptr_t),
            ("intmax_t", intmax_t),
            ("uintmax_t", uintmax_t),
            ("off_t", off_t),
            ("time_t", time_t),
            ("wchar_t", wchar_t),
        ] {
            add(&mut names, name, id);
        }

        (store, names)
    }
}

/// Returns the pointer width for the supported LP64 targets.
pub const fn pointer_width() -> usize {
    8
}

/// Rounds `offset` up to the next multiple of `align`.
pub fn align_up(offset: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (offset + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for builtin type spellings and layout helpers.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - The assertions mirror the names php-src's `FFI\CType::getName()` returns.

    use super::*;

    /// Builtin integer spellings resolve to the canonical fixed-width display names.
    #[test]
    fn builtin_names_match_php() {
        let (store, names) = TypeStore::with_builtins();
        for (name, display) in [
            ("int", "int32_t"),
            ("unsigned long", "uint64_t"),
            ("long long", "int64_t"),
            ("size_t", "uint64_t"),
            ("off_t", "int64_t"),
        ] {
            assert_eq!(store.get(names[name]).display, display, "{name}");
        }
    }

    /// Pins the target ABI's `long double` layout: Apple ARM64 is 8/8, other LP64 targets 16/16.
    #[test]
    fn long_double_matches_target_abi() {
        let (store, names) = TypeStore::with_builtins();
        let ty = store.get(names["long double"]);
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            assert_eq!((ty.size, ty.align), (8, 8));
        } else {
            assert_eq!((ty.size, ty.align), (16, 16));
        }
    }

    /// Array display names nest the way php-src prints them.
    #[test]
    fn array_display_nests() {
        let (mut store, names) = TypeStore::with_builtins();
        let inner = store.array(names["int"], Some(2));
        let outer = store.array(inner, Some(2));
        assert_eq!(store.get(outer).display, "int32_t[2][2]");
        assert_eq!(store.get(outer).size, 16);
    }
}
