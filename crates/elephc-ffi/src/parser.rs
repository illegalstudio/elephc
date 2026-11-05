//! Purpose:
//! Parses the C declaration subset PHP's `ext/ffi` accepts, including typedefs, tagged
//! structs/unions/enums, bit-fields, arrays, function pointers, and the GNU/MS attributes
//! that affect layout or spell a C type.
//!
//! Called from:
//! - `crate::context` for `FFI::cdef()`, `FFI::load()`, `FFI::type()`, and `FFI::new()`.
//!
//! Key details:
//! - The lexer is C-only: it understands comments, preprocessor lines (captured for
//!   `FFI_LIB`/`FFI_SCOPE` and object-like constants), C integer literals, and punctuation.
//! - Declarators use the standard "derived tree" composition so `int *p[3]`,
//!   `int (*p)[3]`, and `int (*fp)(int)` produce the C meanings rather than textual ones.
//! - Struct/union layout is computed while parsing. Packed records pack bit-fields
//!   contiguously; ordinary records place bit-fields in aligned storage units.
//! - Error text matches php-src where the PHPT corpus asserts it (incomplete tags,
//!   function returning array, flexible members, read-only locations).

use std::collections::{HashMap, HashSet};

use crate::types::{
    align_up, CType, Field, FuncType, StructType, TagKeyword, TypeId, TypeKind, TypeStore,
    ATTR_CONST, ATTR_GCC_STRUCT, ATTR_INCOMPLETE_TAG, ATTR_MS_STRUCT, ATTR_PACKED,
    ATTR_UNION, TYPE_VOID,
};

/// One C parser diagnostic with the source line php-src reports.
#[derive(Clone, Debug)]
pub struct ParseError {
    /// php-src-style message without the closing ` at line N`.
    pub message: String,
    /// One-based source line.
    pub line: usize,
}

impl ParseError {
    /// Formats the message the prelude turns into `FFI\ParserException`.
    pub fn display(&self) -> String {
        format!("{} at line {}", self.message, self.line)
    }
}

/// One parsed `FFI::cdef()`/`FFI::load()` context surface.
#[derive(Clone, Debug, Default)]
pub struct ParsedContext {
    /// Global names (typedef aliases and tags) visible to type-name parsing.
    pub names: HashMap<String, TypeId>,
    /// Tag names (`struct X`, `union X`, `enum X`) visible to type-name parsing.
    pub tags: HashMap<String, TypeId>,
    /// Function declarations by name.
    pub functions: HashMap<String, TypeId>,
    /// Global variable declarations by name.
    pub globals: HashMap<String, TypeId>,
    /// `#define FFI_SCOPE "..."`, when present.
    pub scope: Option<String>,
    /// `#define FFI_LIB "..."`, when present.
    pub library: Option<String>,
    /// Enum constants and object-like `#define` values visible to array dimensions.
    pub constants: HashMap<String, i64>,
    /// `static` functions: declared for type/pointer use but never resolved as symbols.
    pub internal_functions: HashSet<String>,
    /// `static` globals: declared for type use but never resolved as symbols.
    pub internal_globals: HashSet<String>,
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    kind: TokenKind,
    line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenKind {
    Ident,
    Number,
    String,
    Punct,
    Eof,
}

/// Parsed attributes that affect type/layout.
#[derive(Clone, Debug, Default)]
struct Attrs {
    packed: bool,
    /// Explicit alignment; `Some(None)` is GNU `aligned` with no argument.
    aligned: Option<Option<usize>>,
    /// GCC `mode` attribute selected width, if any.
    mode: Option<Mode>,
    ms_struct: bool,
    gcc_struct: bool,
    /// The declaration carried `const` on the field/type itself.
    is_const: bool,
    /// The declaration began with the `typedef` keyword.
    is_typedef: bool,
    /// The declaration carried C's `static` storage class (internal linkage).
    is_static: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Qi,
    Hi,
    Si,
    Di,
    Sf,
    Df,
}

#[derive(Clone, Debug)]
enum Derived {
    Ident(Option<String>),
    Pointer {
        to: Box<Derived>,
        is_const: bool,
    },
    Array {
        of: Box<Derived>,
        len: Option<usize>,
        star: bool,
    },
    Function {
        of: Box<Derived>,
        params: Vec<TypeId>,
        variadic: bool,
        abi: u32,
    },
}

/// Maximum declarator/constant-expression nesting before the parser diagnoses instead of
/// recursing into a stack overflow.
const MAX_NESTING: usize = 1024;

struct Parser<'a> {
    store: &'a mut TypeStore,
    names: HashMap<String, TypeId>,
    user_typedefs: std::collections::HashSet<String>,
    tags: HashMap<String, TypeId>,
    constants: HashMap<String, i64>,
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    scope: Option<String>,
    library: Option<String>,
    functions: HashMap<String, TypeId>,
    globals: HashMap<String, TypeId>,
    internal_functions: HashSet<String>,
    internal_globals: HashSet<String>,
    allow_star_array: bool,
}

/// Parses a C declaration block against `store`, starting from `base_names`.
pub fn parse_context(
    store: &mut TypeStore,
    base_names: &HashMap<String, TypeId>,
    code: &str,
) -> Result<ParsedContext, ParseError> {
    let (clean, defines, scope, library) = preprocess(code);
    let tokens = tokenize(&clean)?;
    let mut parser = Parser {
        store,
        names: base_names.clone(),
        user_typedefs: std::collections::HashSet::new(),
        tags: HashMap::new(),
        constants: defines,
        tokens,
        pos: 0,
        depth: 0,
        scope,
        library,
        functions: HashMap::new(),
        globals: HashMap::new(),
        internal_functions: HashSet::new(),
        internal_globals: HashSet::new(),
        allow_star_array: false,
    };
    parser.parse_declarations()?;
    Ok(ParsedContext {
        names: parser.names,
        tags: parser.tags,
        functions: parser.functions,
        globals: parser.globals,
        internal_functions: parser.internal_functions,
        internal_globals: parser.internal_globals,
        scope: parser.scope,
        library: parser.library,
        constants: parser.constants,
    })
}

/// Parses one type name (the argument to `FFI::type()`/`FFI::new()`/`FFI::cast()`).
///
/// `context_names` are the typedef/tag names visible in the calling FFI context and
/// `base_names` are the builtin spellings.
pub fn parse_type_name(
    store: &mut TypeStore,
    base_names: &HashMap<String, TypeId>,
    context_names: &HashMap<String, TypeId>,
    context_tags: &HashMap<String, TypeId>,
    context_constants: &HashMap<String, i64>,
    code: &str,
) -> Result<TypeId, ParseError> {
    let (ty, _, _) = parse_type_name_with_names(
        store,
        base_names,
        context_names,
        context_tags,
        context_constants,
        code,
    )?;
    Ok(ty)
}

/// Parses a type name and returns both the type and any tags/typedefs it introduced.
pub fn parse_type_name_with_names(
    store: &mut TypeStore,
    base_names: &HashMap<String, TypeId>,
    context_names: &HashMap<String, TypeId>,
    context_tags: &HashMap<String, TypeId>,
    context_constants: &HashMap<String, i64>,
    code: &str,
) -> Result<(TypeId, HashMap<String, TypeId>, HashMap<String, TypeId>), ParseError> {
    let (clean, defines, _, _) = preprocess(code);
    let tokens = tokenize(&clean)?;
    let mut parser = Parser {
        store,
        names: base_names.clone(),
        user_typedefs: std::collections::HashSet::new(),
        tags: context_tags.clone(),
        constants: {
            let mut constants = context_constants.clone();
            constants.extend(defines);
            constants
        },
        tokens,
        pos: 0,
        depth: 0,
        scope: None,
        library: None,
        functions: HashMap::new(),
        globals: HashMap::new(),
        internal_functions: HashSet::new(),
        internal_globals: HashSet::new(),
        allow_star_array: false,
    };
    parser.names.extend(context_names.iter().map(|(key, value)| (key.clone(), *value)));
    let ty = parser.parse_type_name_inner()?;
    if !parser.at_eof() {
        return Err(parser.error("Unexpected token"));
    }
    if parser.type_has_incomplete_array(ty) {
        return Err(parser.error("\"[]\" is not allowed"));
    }
    Ok((ty, parser.names, parser.tags))
}

impl<'a> Parser<'a> {
    /// Builds a parser diagnostic at the current token line.
    fn error(&self, message: &str) -> ParseError {
        let line = if self.peek().kind == TokenKind::Eof {
            self.tokens[self.pos.saturating_sub(1)].line
        } else {
            self.peek().line
        };
        ParseError {
            message: message.to_string(),
            line,
        }
    }

    /// Builds a parser diagnostic at one token line.
    fn error_on(&self, token: &Token, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            line: token.line,
        }
    }

    /// Returns the current token without consuming it.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    /// Returns the current token text.
    fn peek_text(&self) -> &str {
        &self.peek().text
    }

    /// Consumes and returns the current token, stopping at EOF.
    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    /// Returns whether the token stream reached EOF.
    fn at_eof(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    /// Consumes the current token when it matches `text`.
    fn eat(&mut self, text: &str) -> bool {
        if self.peek_text() == text {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes `text` or returns the C parser diagnostic.
    fn expect(&mut self, text: &str) -> Result<Token, ParseError> {
        if self.peek_text() == text {
            Ok(self.bump())
        } else {
            Err(self.error(&format!("Expected \"{}\"", text)))
        }
    }

    /// Consumes and returns an identifier token, or `None`.
    fn eat_ident(&mut self) -> Option<String> {
        if self.peek().kind == TokenKind::Ident {
            Some(self.bump().text)
        } else {
            None
        }
    }

    /// Parses every top-level declaration in the token stream.
    fn parse_declarations(&mut self) -> Result<(), ParseError> {
        while !self.at_eof() {
            if self.eat(";") {
                continue;
            }
            self.parse_declaration()?;
        }
        Ok(())
    }

    /// Parses one declaration, adding functions, globals, or typedefs to the context.
    fn parse_declaration(&mut self) -> Result<(), ParseError> {
        let mut attrs = Attrs::default();
        let base = self.parse_specifiers(&mut attrs)?;
        if self.eat(";") {
            return Ok(());
        }
        loop {
            let derived = self.parse_declarator()?;
            let field_attrs = self.parse_trailing_attrs()?;
            let base = self.apply_base_const(base, &attrs)?;
            let mut ty = self.apply(base, &derived)?;
            let mut base_attrs = attrs.clone();
            base_attrs.is_const = false;
            ty = self.apply_type_attrs(ty, &base_attrs)?;
            ty = self.apply_type_attrs(ty, &field_attrs)?;
            let name = derived_name(&derived);
            if attrs.is_typedef {
                let Some(name) = name else {
                    return Err(self.error("Typedef name required"));
                };
                if let Some(existing) = self.names.get(&name).copied() {
                    if self.user_typedefs.contains(&name)
                        && !self.types_equivalent(existing, ty)
                    {
                        return Err(self.error(&format!("Redeclaration of \"{}\"", name)));
                    }
                }
                self.user_typedefs.insert(name.clone());
                self.names.insert(name, ty);
            } else if matches!(self.store.get(ty).kind, TypeKind::Func(_)) {
                let Some(name) = name else {
                    return Err(self.error("Function name required"));
                };
                if attrs.is_static {
                    self.internal_functions.insert(name.clone());
                }
                self.functions.insert(name, ty);
            } else {
                self.ensure_complete_value_type(ty)?;
                if let Some(name) = name {
                    if attrs.is_static {
                        self.internal_globals.insert(name.clone());
                    }
                    self.globals.insert(name, ty);
                }
            }
            if self.eat(",") {
                continue;
            }
            self.expect(";")?;
            return Ok(());
        }
    }

    /// Compares two resolved types structurally for C typedef redeclaration rules.
    fn types_equivalent(&self, left: TypeId, right: TypeId) -> bool {
        let left_ty = self.store.get(left);
        let right_ty = self.store.get(right);
        left_ty.display == right_ty.display
            && left_ty.size == right_ty.size
            && left_ty.align == right_ty.align
            && matches!(
                (&left_ty.kind, &right_ty.kind),
                (TypeKind::Void, TypeKind::Void)
                    | (TypeKind::Bool, TypeKind::Bool)
                    | (TypeKind::Char { .. }, TypeKind::Char { .. })
                    | (TypeKind::Int { .. }, TypeKind::Int { .. })
                    | (TypeKind::Float { .. }, TypeKind::Float { .. })
                    | (TypeKind::Pointer(_), TypeKind::Pointer(_))
                    | (TypeKind::Array { .. }, TypeKind::Array { .. })
                    | (TypeKind::Struct(_), TypeKind::Struct(_))
                    | (TypeKind::Enum { .. }, TypeKind::Enum { .. })
                    | (TypeKind::Func(_), TypeKind::Func(_))
            )
    }

    /// Returns whether a type contains an incomplete array member.
    fn type_has_incomplete_array(&self, ty: TypeId) -> bool {
        match &self.store.get(ty).kind {
            TypeKind::Array { len: None, .. } => true,
            TypeKind::Array { element, .. } => self.type_has_incomplete_array(*element),
            TypeKind::Struct(record) => record
                .fields
                .iter()
                .any(|field| self.type_has_incomplete_array(field.ty)),
            _ => false,
        }
    }

    /// Returns whether a struct/union contains a flexible array member.
    fn type_has_flexible_array(&self, ty: TypeId) -> bool {
        match &self.store.get(ty).kind {
            TypeKind::Array { len: None, .. } => true,
            TypeKind::Struct(record) => record
                .fields
                .iter()
                .any(|field| self.type_has_flexible_array(field.ty)),
            _ => false,
        }
    }

    /// Rejects a by-value use of an incomplete tag in a C declaration.
    fn ensure_complete_value_type(&self, ty: TypeId) -> Result<(), ParseError> {
        match &self.store.get(ty).kind {
            TypeKind::IncompleteTag { keyword, tag } => Err(self.error(&format!(
                "Incomplete {} \"{}\"",
                keyword.as_str(),
                tag
            ))),
            TypeKind::Array { element, .. } => self.ensure_complete_value_type(*element),
            TypeKind::Struct(record) if record.incomplete => Err(self.error("Incomplete struct")),
            TypeKind::Enum { incomplete: true, .. } => Err(self.error("Incomplete enum")),
            TypeKind::Pointer(_) | TypeKind::Func(_) => Ok(()),
            _ => Ok(()),
        }
    }

    /// Parses one type name inside an existing context.
    fn parse_type_name_inner(&mut self) -> Result<TypeId, ParseError> {
        let mut attrs = Attrs::default();
        let base = self.parse_specifiers(&mut attrs)?;
        let base = self.apply_base_const(base, &attrs)?;
        let mut base_attrs = attrs.clone();
        base_attrs.is_const = false;
        let base = self.apply_type_attrs(base, &base_attrs)?;
        if self.at_eof() {
            return Ok(base);
        }
        let derived = self.parse_declarator()?;
        self.apply(base, &derived)
    }

    /// Parses declaration specifiers and records their attributes.
    fn parse_specifiers(&mut self, attrs: &mut Attrs) -> Result<TypeId, ParseError> {
        let mut signed: Option<bool> = None;
        let mut short = false;
        let mut long_count = 0u8;
        let mut base: Option<TypeId> = None;
        let mut saw_any = false;

        loop {
            let text = self.peek_text().to_string();
            match text.as_str() {
                "typedef" => {
                    self.bump();
                    attrs.is_typedef = true;
                    saw_any = true;
                }
                "static" | "auto" | "register" => {
                    self.bump();
                    attrs.is_static = true;
                    saw_any = true;
                }
                "extern" | "inline" | "_Noreturn" | "__extension__"
                | "__cdecl" | "__stdcall" | "__fastcall" | "__vectorcall" | "__thiscall"
                | "__pascal" => {
                    self.bump();
                    saw_any = true;
                }
                "const" => {
                    self.bump();
                    attrs.is_const = true;
                    saw_any = true;
                }
                "volatile" | "restrict" | "_Atomic" => {
                    self.bump();
                    saw_any = true;
                }
                "signed" => {
                    self.bump();
                    signed = Some(true);
                    saw_any = true;
                }
                "unsigned" => {
                    self.bump();
                    signed = Some(false);
                    saw_any = true;
                }
                "short" => {
                    self.bump();
                    short = true;
                    saw_any = true;
                }
                "long" => {
                    self.bump();
                    long_count = long_count.saturating_add(1);
                    saw_any = true;
                }
                "void" | "_Bool" | "char" | "int" | "float" | "double" => {
                    self.bump();
                    base = Some(self.primitive_specifier(&text, signed, short, long_count)?);
                    saw_any = true;
                    break;
                }
                "struct" | "union" | "enum" => {
                    let keyword = match text.as_str() {
                        "struct" => TagKeyword::Struct,
                        "union" => TagKeyword::Union,
                        _ => TagKeyword::Enum,
                    };
                    base = Some(self.parse_tag_specifier(keyword)?);
                    saw_any = true;
                    break;
                }
                "_Alignas" => {
                    self.parse_alignas(attrs)?;
                    saw_any = true;
                }
                "__attribute__" | "__declspec" => {
                    self.parse_attribute(attrs)?;
                    saw_any = true;
                }
                _ => {
                    if self.peek().kind == TokenKind::Ident {
                        if let Some(id) = self.names.get(&text).copied() {
                            self.bump();
                            base = Some(id);
                            saw_any = true;
                            break;
                        }
                        if base.is_none()
                            && !short
                            && long_count == 0
                            && signed.is_none()
                        {
                            return Err(self.error(&format!("Undefined C type \"{}\"", text)));
                        }
                    }
                    break;
                }
            }
        }

        if !saw_any {
            return Err(self.error("Expected type"));
        }
        // C allows type qualifiers and attributes after the base specifier (`int const`).
        loop {
            match self.peek_text() {
                "const" => {
                    self.bump();
                    attrs.is_const = true;
                }
                "volatile" | "restrict" | "_Atomic" => {
                    self.bump();
                }
                "_Alignas" => self.parse_alignas(attrs)?,
                "__attribute__" | "__declspec" => self.parse_attribute(attrs)?,
                _ => break,
            }
        }
        let base = match base {
            Some(base) => base,
            None => {
                if short {
                    self.store.int(16, signed.unwrap_or(true), if signed == Some(false) { "uint16_t" } else { "int16_t" })
                } else if long_count >= 2 {
                    self.store.int(64, signed.unwrap_or(true), if signed == Some(false) { "uint64_t" } else { "int64_t" })
                } else if long_count == 1 {
                    self.store.int(64, signed.unwrap_or(true), if signed == Some(false) { "uint64_t" } else { "int64_t" })
                } else if signed.is_some() {
                    self.store.int(32, signed.unwrap(), if signed == Some(false) { "uint32_t" } else { "int32_t" })
                } else {
                    self.store.int(32, true, "int32_t")
                }
            }
        };
        if attrs.mode.is_some() {
            return self.apply_mode(base, attrs.mode);
        }
        Ok(base)
    }

    /// Builds the primitive type for one C keyword.
    fn primitive_specifier(
        &mut self,
        text: &str,
        signed: Option<bool>,
        short: bool,
        long_count: u8,
    ) -> Result<TypeId, ParseError> {
        let id = match text {
            "void" => {
                if signed.is_some() || short || long_count > 0 {
                    return Err(self.error("Invalid type specifier"));
                }
                self.store.push(CType {
                    kind: TypeKind::Void,
                    size: 0,
                    align: 1,
                    attrs: 0,
                    display: "void".to_string(),
                    is_const: false,
                })
            }
            "_Bool" => self.store.push(CType {
                kind: TypeKind::Bool,
                size: 1,
                align: 1,
                attrs: 0,
                display: "bool".to_string(),
                is_const: false,
            }),
            "char" => {
                // php-src reserves TYPE_CHAR (and one-byte-string reads) for bare `char`.
                // `signed char`/`unsigned char` are explicit 8-bit integers (int8_t/uint8_t).
                if let Some(signed) = signed {
                    self.store.int(8, signed, if signed { "int8_t" } else { "uint8_t" })
                } else {
                    self.store.push(CType {
                        kind: TypeKind::Char { signed: crate::types::char_is_signed() },
                        size: 1,
                        align: 1,
                        attrs: 0,
                        display: "char".to_string(),
                        is_const: false,
                    })
                }
            }
            "int" => {
                if short {
                    self.store.int(16, signed.unwrap_or(true), if signed == Some(false) { "uint16_t" } else { "int16_t" })
                } else if long_count > 0 {
                    self.store.int(64, signed.unwrap_or(true), if signed == Some(false) { "uint64_t" } else { "int64_t" })
                } else {
                    self.store.int(32, signed.unwrap_or(true), if signed == Some(false) { "uint32_t" } else { "int32_t" })
                }
            }
            "float" => {
                if long_count > 0 {
                    return Err(self.error("Invalid type specifier"));
                }
                self.store.float(32, "float", 4, 4)
            }
            "double" => {
                if long_count > 0 {
                    let (size, align) = long_double_layout();
                    self.store.float(80, "long double", size, align)
                } else {
                    self.store.float(64, "double", 8, 8)
                }
            }
            _ => unreachable!("primitive specifier"),
        };
        Ok(id)
    }

    /// Parses a struct, union, or enum specifier and its body.
    fn parse_tag_specifier(&mut self, keyword: TagKeyword) -> Result<TypeId, ParseError> {
        self.expect(keyword.as_str())?;
        let mut attrs = Attrs::default();
        loop {
            if self.peek_text() == "__attribute__" || self.peek_text() == "__declspec" {
                self.parse_attribute(&mut attrs)?;
            } else if self.peek_text() == "_Alignas" {
                self.parse_alignas(&mut attrs)?;
            } else {
                break;
            }
        }
        let tag = self.eat_ident();
        let has_body = self.peek_text() == "{";
        if !has_body {
            let Some(tag) = tag else {
                return Err(self.error("Tag name required"));
            };
            return self.reference_tag(keyword, &tag);
        }
        let id = self.parse_tag_definition(keyword, tag, attrs)?;
        let trailing = self.parse_trailing_attrs()?;
        self.apply_type_attrs(id, &trailing)
    }

    /// Resolves or creates the incomplete tag referenced by a declaration.
    fn reference_tag(&mut self, keyword: TagKeyword, tag: &str) -> Result<TypeId, ParseError> {
        let key = format!("{} {}", keyword.as_str(), tag);
        if let Some(id) = self.tags.get(&key) {
            return Ok(*id);
        }
        let display = format!("{} {}", keyword.as_str(), tag);
        let kind = TypeKind::IncompleteTag {
            keyword,
            tag: tag.to_string(),
        };
        let attr = match keyword {
            TagKeyword::Enum => ATTR_INCOMPLETE_TAG,
            _ => ATTR_INCOMPLETE_TAG,
        };
        let id = self.store.push(CType {
            kind,
            size: 0,
            align: 1,
            attrs: attr,
            display,
            is_const: false,
        });
        self.tags.insert(key, id);
        Ok(id)
    }

    /// Parses and finalizes one tagged struct, union, or enum definition.
    fn parse_tag_definition(
        &mut self,
        keyword: TagKeyword,
        tag: Option<String>,
        attrs: Attrs,
    ) -> Result<TypeId, ParseError> {
        let key = tag.as_ref().map(|tag| format!("{} {}", keyword.as_str(), tag));
        let existing = key.as_ref().and_then(|key| self.tags.get(key).copied());
        if let Some(existing) = existing {
            if !matches!(self.store.get(existing).kind, TypeKind::IncompleteTag { .. }) {
                return Err(self.error(&format!("Redeclaration of \"{}\"", tag.as_deref().unwrap_or(""))));
            }
        }
        match keyword {
            TagKeyword::Struct | TagKeyword::Union => {
                let id = existing.unwrap_or_else(|| self.allocate_incomplete(keyword, tag.as_deref()));
                // Register the tag before its body: `struct Node { struct Node *next; }` must
                // reference this same type, not allocate an orphaned incomplete tag.
                if let Some(key) = key {
                    self.tags.insert(key, id);
                }
                self.parse_struct_body(id, keyword == TagKeyword::Union, attrs, tag.clone())?;
                Ok(id)
            }
            TagKeyword::Enum => {
                let id = existing.unwrap_or_else(|| self.allocate_incomplete(keyword, tag.as_deref()));
                if let Some(key) = key {
                    self.tags.insert(key, id);
                }
                self.parse_enum_body(id, attrs, tag.clone())?;
                Ok(id)
            }
        }
    }

    /// Allocates an incomplete tag placeholder for forward references.
    fn allocate_incomplete(&mut self, keyword: TagKeyword, tag: Option<&str>) -> TypeId {
        let display = match tag {
            Some(tag) => format!("{} {}", keyword.as_str(), tag),
            None => format!("{} <anonymous>", keyword.as_str()),
        };
        self.store.push(CType {
            kind: TypeKind::IncompleteTag {
                keyword,
                tag: tag.unwrap_or_default().to_string(),
            },
            size: 0,
            align: 1,
            attrs: ATTR_INCOMPLETE_TAG,
            display,
            is_const: false,
        })
    }

    /// Parses struct/union members and computes their layout.
    fn parse_struct_body(
        &mut self,
        id: TypeId,
        union: bool,
        attrs: Attrs,
        tag: Option<String>,
    ) -> Result<(), ParseError> {
        self.expect("{")?;
        let mut raw = Vec::new();
        while !self.eat("}") {
            let mut field_attrs = Attrs::default();
            let base = self.parse_specifiers(&mut field_attrs)?;
            let base = self.apply_base_const(base, &field_attrs)?;
            let mut field_type_attrs = field_attrs.clone();
            field_type_attrs.is_const = false;
            if self.eat(";") {
                if matches!(self.store.get(base).kind, TypeKind::Struct(_) | TypeKind::IncompleteTag { .. }) {
                    raw.push(RawField {
                        name: None,
                        ty: base,
                        bit_width: None,
                        attrs: field_attrs,
                        anonymous: true,
                    });
                }
                continue;
            }
            loop {
                let derived = self.parse_declarator()?;
                let mut ty = self.apply(base, &derived)?;
                ty = self.apply_type_attrs(ty, &field_type_attrs)?;
                let trailing = self.parse_trailing_attrs()?;
                let mut combined = field_attrs.clone();
                combined.packed |= trailing.packed;
                combined.ms_struct |= trailing.ms_struct;
                combined.gcc_struct |= trailing.gcc_struct;
                combined.is_const |= trailing.is_const;
                if trailing.aligned.is_some() {
                    combined.aligned = trailing.aligned;
                }
                if trailing.mode.is_some() {
                    combined.mode = trailing.mode;
                    ty = self.apply_mode(ty, trailing.mode)?;
                }
                let bit_width = if self.eat(":") {
                    let width = self.parse_const_expr()?;
                    if width < 0 || width > 255 {
                        return Err(self.error("Invalid bit-field width"));
                    }
                    Some(width as u8)
                } else {
                    None
                };
                let name = derived_name(&derived);
                let anonymous = name.is_none()
                    && bit_width.is_none()
                    && matches!(
                        self.store.get(ty).kind,
                        TypeKind::Struct(_) | TypeKind::IncompleteTag { .. }
                    );
                raw.push(RawField {
                    name,
                    ty,
                    bit_width,
                    attrs: combined,
                    anonymous,
                });
                if self.eat(",") {
                    continue;
                }
                if self.peek_text() == "}" {
                    break;
                }
                self.expect(";")?;
                break;
            }
        }

        let natural = self.layout_record(&raw, union, attrs.packed, tag.as_deref())?;
        let mut record_attrs = if union { ATTR_UNION } else { 0 };
        if attrs.packed {
            record_attrs |= ATTR_PACKED;
        }
        if attrs.ms_struct {
            record_attrs |= ATTR_MS_STRUCT;
        }
        if attrs.gcc_struct {
            record_attrs |= ATTR_GCC_STRUCT;
        }
        let display = match &tag {
            Some(tag) => format!("{} {}", if union { "union" } else { "struct" }, tag),
            None => format!("{} <anonymous>", if union { "union" } else { "struct" }),
        };
        let mut size = natural.size;
        let mut align = natural.align;
        if let Some(requested) = attrs.aligned {
            let requested = requested.unwrap_or_else(max_alignment);
            align = align.max(requested);
            size = align_up(size, align);
        }
        if attrs.packed && attrs.aligned.is_none() {
            align = 1;
            size = align_up(natural.bits.div_ceil(8), 1);
        }
        size = align_up(size.max(natural.bits.div_ceil(8)), align);
        self.store.replace(
            id,
            CType {
                kind: TypeKind::Struct(StructType {
                    fields: natural.fields,
                    union,
                    packed: attrs.packed,
                    tag: tag.clone(),
                    incomplete: false,
                }),
                size,
                align,
                attrs: record_attrs,
                display,
                is_const: false,
            },
        );
        Ok(())
    }

    /// Lays out a struct or union, including packed and bit-field storage.
    fn layout_record(
        &mut self,
        raw: &[RawField],
        union: bool,
        packed: bool,
        current_tag: Option<&str>,
    ) -> Result<NaturalLayout, ParseError> {
        let mut fields = Vec::new();
        let mut byte_size = 0usize;
        let mut align = 1usize;
        // php-src continues every consecutive bit-field in the same 32-bit allocation unit
        // (`first_bit = previous.first_bit + previous.bits`), even when that unit must grow past
        // four bytes. A non-bit-field resets the chain.
        let mut previous_bit_field: Option<(usize, usize, usize)> = None;

        for (index, field) in raw.iter().enumerate() {
            let ty = self.store.get(field.ty).clone();
            match &ty.kind {
                TypeKind::Func(_) => {
                    return Err(self.error("function type is not allowed"));
                }
                TypeKind::IncompleteTag { keyword, tag } => {
                    if current_tag.is_some_and(|current| current == tag) {
                        return Err(self.error("Struct/union can't contain an instance of itself"));
                    }
                    return Err(self.error(&format!("Incomplete {} \"{}\"", keyword.as_str(), tag)));
                }
                TypeKind::Array { element, .. } => {
                    if let TypeKind::IncompleteTag { keyword, tag } = &self.store.get(*element).kind {
                        return Err(self.error(&format!("Incomplete {} \"{}\"", keyword.as_str(), tag)));
                    }
                }
                TypeKind::Struct(record) if record.incomplete => {
                    return Err(self.error("Incomplete struct"));
                }
                TypeKind::Enum { incomplete: true, .. } => {
                    return Err(self.error("Incomplete enum"));
                }
                _ => {}
            }
            let is_flexible = matches!(ty.kind, TypeKind::Array { len: None, .. })
                || self.type_has_flexible_array(field.ty);
            if is_flexible {
                if union {
                    return Err(self.error("Flexible array member in union"));
                }
                if index + 1 != raw.len() {
                    return Err(self.error("Flexible array member not at end of struct"));
                }
            }
            let readonly = ty.is_const
                || matches!(ty.kind, TypeKind::Array { element, .. } if self.store.get(element).is_const);
            let requested_align = match field.attrs.aligned {
                Some(Some(value)) => Some(value),
                Some(None) => Some(max_alignment()),
                None => None,
            };
            let field_packed = packed || field.attrs.packed;
            let natural_align = if field_packed { 1 } else { ty.align.max(1) };
            let field_align = requested_align
                .map(|requested| natural_align.max(requested))
                .unwrap_or(natural_align);
            if field.bit_width.is_some() {
                if !ty.is_integer_like() {
                    return Err(self.error("Bit-field has invalid type"));
                }
                let width = field.bit_width.unwrap_or(0) as usize;
                if width == 0 {
                    // php-src rejects a named zero-width bit-field and ignores an anonymous one.
                    if field.name.is_some() {
                        return Err(self.error("Zero width in bit-field"));
                    }
                    continue;
                }
                if width > ty.size * 8 {
                    return Err(self.error("Invalid bit-field width"));
                }
                // php-src allocates every bit-field in a `uint32_t` unit unless the record is
                // packed; the declared field type does not choose the unit.
                align = align.max(if packed { 1 } else { 4 });
                let (offset, first_bit, total_bits) = if union {
                    (0, 0, width)
                } else if let Some((previous_offset, previous_first, previous_width)) = previous_bit_field {
                    let first = previous_first + previous_width;
                    (previous_offset, first, first + width)
                } else {
                    (byte_size, 0, width)
                };
                let field_size = if packed {
                    total_bits.div_ceil(8)
                } else {
                    total_bits.div_ceil(32) * 4
                };
                if union {
                    byte_size = byte_size.max(field_size);
                } else {
                    byte_size = byte_size.max(offset + field_size);
                }
                previous_bit_field = Some((offset, first_bit, width));
                fields.push(Field {
                    name: field.name.clone(),
                    ty: field.ty,
                    offset,
                    bit_offset: first_bit as u16,
                    bit_width: field.bit_width,
                    readonly,
                });
            } else {
                previous_bit_field = None;
                let offset = if union {
                    0
                } else if field_packed {
                    byte_size
                } else {
                    align_up(byte_size, field_align)
                };
                if union {
                    byte_size = byte_size.max(ty.size);
                } else {
                    byte_size = offset + ty.size;
                }
                align = align.max(if field_packed { 1 } else { field_align });
                fields.push(Field {
                    name: field.name.clone(),
                    ty: field.ty,
                    offset,
                    bit_offset: 0,
                    bit_width: None,
                    readonly,
                });
            }
        }
        let size = if packed { byte_size } else { align_up(byte_size, align) };
        Ok(NaturalLayout {
            fields,
            bits: size * 8,
            size,
            align,
        })
    }

    /// Parses enum members and installs their integer constants.
    fn parse_enum_body(&mut self, id: TypeId, attrs: Attrs, tag: Option<String>) -> Result<(), ParseError> {
        self.expect("{")?;
        let mut values = Vec::new();
        let mut next = 0i64;
        while !self.eat("}") {
            let Some(name) = self.eat_ident() else {
                return Err(self.error("Enum constant name required"));
            };
            let value = if self.eat("=") {
                self.parse_const_expr()?
            } else {
                next
            };
            next = value.saturating_add(1);
            self.constants.insert(name.clone(), value);
            values.push((name, value));
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
        }
        let (size, signed) = enum_layout(&values, attrs.packed);
        let underlying = self.store.int(
            (size * 8) as u8,
            signed,
            if signed {
                match size {
                    1 => "int8_t",
                    2 => "int16_t",
                    4 => "int32_t",
                    _ => "int64_t",
                }
            } else {
                match size {
                    1 => "uint8_t",
                    2 => "uint16_t",
                    4 => "uint32_t",
                    _ => "uint64_t",
                }
            },
        );
        let mut enum_attrs = if attrs.packed { ATTR_PACKED } else { 0 };
        if attrs.packed {
            enum_attrs |= ATTR_PACKED;
        }
        let display = match tag {
            Some(tag) => format!("enum {}", tag),
            None => "enum <anonymous>".to_string(),
        };
        self.store.replace(
            id,
            CType {
                kind: TypeKind::Enum {
                    underlying,
                    values,
                    incomplete: false,
                },
                size,
                align: size,
                attrs: enum_attrs,
                display,
                is_const: false,
            },
        );
        Ok(())
    }

    /// Parses one C declarator into its derived-type tree.
    fn parse_declarator(&mut self) -> Result<Derived, ParseError> {
        if self.depth >= MAX_NESTING {
            return Err(self.error("declaration nesting too deep"));
        }
        self.depth += 1;
        let result = self.parse_declarator_unchecked();
        self.depth -= 1;
        result
    }

    /// Parses one declarator after the nesting guard has been applied.
    fn parse_declarator_unchecked(&mut self) -> Result<Derived, ParseError> {
        let mut pointers = Vec::new();
        while self.eat("*") {
            let mut is_const = false;
            loop {
                match self.peek_text() {
                    "const" => {
                        self.bump();
                        is_const = true;
                    }
                    "volatile" | "restrict" | "_Atomic" | "__cdecl" | "__stdcall"
                    | "__fastcall" | "__vectorcall" | "__thiscall" | "__pascal" => {
                        self.bump();
                    }
                    _ => break,
                }
            }
            let _ = self.parse_trailing_attrs()?;
            pointers.push(is_const);
        }

        let mut derived = if self.eat("(") {
            let inner = self.parse_declarator()?;
            self.expect(")")?;
            inner
        } else if self.peek().kind == TokenKind::Ident {
            let name = self.bump().text;
            Derived::Ident(Some(name))
        } else {
            Derived::Ident(None)
        };

        loop {
            if self.eat("[") {
                let mut star = false;
                loop {
                    match self.peek_text() {
                        "const" | "volatile" | "restrict" | "static" | "_Atomic" => {
                            self.bump();
                        }
                        "*" => {
                            self.bump();
                            star = true;
                        }
                        _ => break,
                    }
                }
                let len = if self.eat("]") {
                    None
                } else {
                    let value = self.parse_const_expr()?;
                    if value < 0 {
                        return Err(self.error("Invalid array size"));
                    }
                    self.expect("]")?;
                    Some(value as usize)
                };
                derived = Derived::Array {
                    of: Box::new(derived),
                    len,
                    star,
                };
            } else if self.eat("(") {
                let (params, variadic, abi) = self.parse_parameters()?;
                derived = Derived::Function {
                    of: Box::new(derived),
                    params,
                    variadic,
                    abi,
                };
            } else {
                break;
            }
        }

        for is_const in pointers.into_iter().rev() {
            derived = Derived::Pointer {
                to: Box::new(derived),
                is_const,
            };
        }
        Ok(derived)
    }

    /// Parses a function parameter list, including variadics and ABI attrs.
    fn parse_parameters(&mut self) -> Result<(Vec<TypeId>, bool, u32), ParseError> {
        let mut params = Vec::new();
        let mut variadic = false;
        if self.eat(")") {
            return Ok((params, false, 0));
        }
        loop {
            if self.eat("...") {
                variadic = true;
                self.expect(")")?;
                break;
            }
            let mut attrs = Attrs::default();
            let base = self.parse_specifiers(&mut attrs)?;
            let base = self.apply_base_const(base, &attrs)?;
            let mut base_attrs = attrs.clone();
            base_attrs.is_const = false;
            if self.peek_text() == ")" || self.peek_text() == "," {
                let ty = self.apply_type_attrs(base, &base_attrs)?;
                params.push(self.adjust_parameter(ty));
                if self.eat(",") {
                    continue;
                }
                self.expect(")")?;
                break;
            }
            let previous_allow_star = self.allow_star_array;
            self.allow_star_array = true;
            let derived = self.parse_declarator()?;
            let ty = self.apply(base, &derived)?;
            self.allow_star_array = previous_allow_star;
            let ty = self.apply_type_attrs(ty, &base_attrs)?;
            let _ = self.parse_trailing_attrs()?;
            params.push(self.adjust_parameter(ty));
            if self.eat(",") {
                continue;
            }
            self.expect(")")?;
            break;
        }
        if params.len() == 1 && self.store.get(params[0]).kind_code() == TYPE_VOID {
            params.clear();
        }
        Ok((params, variadic, 0))
    }

    /// Applies C parameter decay and default promotions.
    fn adjust_parameter(&mut self, ty: TypeId) -> TypeId {
        match self.store.get(ty).clone().kind {
            TypeKind::Array { element, .. } => self.store.pointer(element),
            TypeKind::Func(func) => {
                let func_id = self.store.function(func);
                self.store.pointer(func_id)
            }
            _ => ty,
        }
    }

    /// Applies the declaration specifiers' `const` to the base type, before a pointer/array
    /// declarator wraps it. `const int *p` is a pointer to a read-only integer; applying the
    /// qualifier after `apply()` made the POINTER const instead.
    fn apply_base_const(&mut self, base: TypeId, attrs: &Attrs) -> Result<TypeId, ParseError> {
        if !attrs.is_const || self.store.get(base).is_const {
            return Ok(base);
        }
        let mut clone = self.store.get(base).clone();
        clone.is_const = true;
        Ok(self.store.push(clone))
    }

    /// Builds one derived type by applying a declarator to its base type.
    fn apply(&mut self, base: TypeId, derived: &Derived) -> Result<TypeId, ParseError> {
        match derived {
            Derived::Ident(_) => Ok(base),
            Derived::Pointer { to, is_const } => {
                let base_is_const = self.store.get(base).is_const;
                let pointer = self.store.pointer(base);
                if base_is_const {
                    // php-src stores the pointee's `const` on the derived pointer's attribute
                    // word: `FFI::type('const int *')->getAttributes()` has ATTR_CONST.
                    self.store.get_mut(pointer).attrs |= ATTR_CONST;
                }
                if *is_const {
                    self.store.get_mut(pointer).is_const = true;
                }
                self.apply(pointer, to)
            }
            Derived::Array { of, len, star } => {
                if *star && !self.allow_star_array {
                    return Err(self.error("\"[*]\" is not allowed in other than function prototype scope"));
                }
                if matches!(self.store.get(base).kind, TypeKind::Func(_)) {
                    return Err(self.error("Array of functions is not allowed"));
                }
                if len.is_some()
                    && matches!(
                        self.store.get(base).kind,
                        TypeKind::Array { len: None, .. }
                    )
                {
                    return Err(self.error("Only the leftmost array can be undimensioned"));
                }
                let base_is_const = self.store.get(base).is_const;
                let array = self.store.array(base, *len);
                if base_is_const {
                    // Arrays follow the same php-src attribute rule as pointers.
                    self.store.get_mut(array).attrs |= ATTR_CONST;
                }
                self.apply(array, of)
            }
            Derived::Function {
                of,
                params,
                variadic,
                abi,
            } => {
                if matches!(self.store.get(base).kind, TypeKind::Array { .. }) {
                    return Err(self.error("Function returning array is not allowed"));
                }
                if matches!(self.store.get(base).kind, TypeKind::Func(_)) {
                    return Err(self.error("Function returning function is not allowed"));
                }
                let func = self.store.function(FuncType {
                    ret: base,
                    params: params.clone(),
                    variadic: *variadic,
                    abi: *abi,
                });
                self.apply(func, of)
            }
        }
    }

    /// Applies const, mode, alignment, and packing attributes to a type.
    fn apply_type_attrs(&mut self, ty: TypeId, attrs: &Attrs) -> Result<TypeId, ParseError> {
        let mut ty = ty;
        if let Some(mode) = attrs.mode {
            ty = self.apply_mode(ty, Some(mode))?;
        }
        if attrs.is_const {
            if self.store.get(ty).is_const {
                // Already const-qualified: keep the existing type.
            } else {
                let mut clone = self.store.get(ty).clone();
                clone.is_const = true;
                ty = self.store.push(clone);
            }
        }
        if let Some(aligned) = attrs.aligned {
            let requested = aligned.unwrap_or_else(max_alignment);
            let mut clone = self.store.get(ty).clone();
            clone.align = clone.align.max(requested);
            clone.size = align_up(clone.size, clone.align);
            ty = self.store.push(clone);
        }
        if attrs.packed {
            self.store.get_mut(ty).attrs |= ATTR_PACKED;
        }
        Ok(ty)
    }

    /// Applies a GCC `mode` attribute while preserving integer signedness.
    fn apply_mode(&mut self, ty: TypeId, mode: Option<Mode>) -> Result<TypeId, ParseError> {
        let Some(mode) = mode else { return Ok(ty) };
        // GCC's `mode` attribute changes the width while preserving the signedness of the
        // declared integer type; forcing `true` turned `unsigned int __attribute__((mode(SI)))`
        // into a signed integer.
        let signed = match self.store.get(ty).kind {
            TypeKind::Int { signed, .. } | TypeKind::Char { signed } => signed,
            TypeKind::Bool => false,
            _ => true,
        };
        let new = match mode {
            Mode::Qi => self.store.int(8, signed, "int8_t"),
            Mode::Hi => self.store.int(16, signed, "int16_t"),
            Mode::Si => self.store.int(32, signed, "int32_t"),
            Mode::Di => self.store.int(64, signed, "int64_t"),
            Mode::Sf => self.store.float(32, "float", 4, 4),
            Mode::Df => self.store.float(64, "double", 8, 8),
        };
        Ok(new)
    }

    /// Parses GNU/MS attributes placed after a declarator.
    fn parse_trailing_attrs(&mut self) -> Result<Attrs, ParseError> {
        let mut attrs = Attrs::default();
        loop {
            match self.peek_text() {
                "__attribute__" | "__declspec" => self.parse_attribute(&mut attrs)?,
                "_Alignas" => self.parse_alignas(&mut attrs)?,
                _ => break,
            }
        }
        Ok(attrs)
    }

    /// Parses a C11 `_Alignas` specifier.
    fn parse_alignas(&mut self, attrs: &mut Attrs) -> Result<(), ParseError> {
        self.expect("_Alignas")?;
        self.expect("(")?;
        let align = if self.peek().kind == TokenKind::Ident {
            let text = self.peek_text().to_string();
            if let Some(id) = self.names.get(&text).copied() {
                self.bump();
                self.store.get(id).align
            } else {
                let value = self.parse_const_expr()?;
                value.max(1) as usize
            }
        } else {
            let value = self.parse_const_expr()?;
            value.max(1) as usize
        };
        self.expect(")")?;
        attrs.aligned = Some(Some(align));
        Ok(())
    }

    /// Parses one GNU `__attribute__` or MS `__declspec` list.
    fn parse_attribute(&mut self, attrs: &mut Attrs) -> Result<(), ParseError> {
        if self.eat("__declspec") {
            self.expect("(")?;
            while !self.eat(")") {
                if self.at_eof() {
                    return Err(self.error("Unterminated attribute"));
                }
                let Some(name) = self.eat_ident() else {
                    self.bump();
                    continue;
                };
                if name == "align" && self.eat("(") {
                    let value = self.parse_const_expr().unwrap_or(0);
                    self.expect(")")?;
                    attrs.aligned = Some(Some(value.max(1) as usize));
                }
            }
            return Ok(());
        }
        self.expect("__attribute__")?;
        self.expect("(")?;
        self.expect("(")?;
        while !self.eat(")") {
            if self.at_eof() {
                return Err(self.error("Unterminated attribute"));
            }
            let Some(name) = self.eat_ident() else {
                self.bump();
                continue;
            };
            match name.as_str() {
                "packed" => attrs.packed = true,
                "ms_struct" => attrs.ms_struct = true,
                "gcc_struct" => attrs.gcc_struct = true,
                "aligned" => {
                    if self.eat("(") {
                        // php-src reports the empty argument as a plain syntax error, but any
                        // other invalid value (unknown name, zero, non-power-of-two) as an
                        // alignment diagnostic instead of silently aligning to one byte.
                        if self.peek_text() == ")" {
                            return Err(self.error("unexpected ')'"));
                        }
                        let value = self
                            .parse_const_expr()
                            .map_err(|_| self.error("Incorrect \"alignment\" value"))?;
                        self.expect(")")?;
                        if value <= 0 || (value & (value - 1)) != 0 {
                            return Err(self.error("Incorrect \"alignment\" value"));
                        }
                        attrs.aligned = Some(Some(value as usize));
                    } else {
                        attrs.aligned = Some(None);
                    }
                }
                "mode" | "__mode__" => {
                    if self.eat("(") {
                        let mode = self.eat_ident();
                        self.expect(")")?;
                        attrs.mode = match mode.as_deref() {
                            Some("__QI__") => Some(Mode::Qi),
                            Some("__HI__") => Some(Mode::Hi),
                            Some("__SI__") => Some(Mode::Si),
                            Some("__DI__") => Some(Mode::Di),
                            Some("__SF__") => Some(Mode::Sf),
                            Some("__DF__") => Some(Mode::Df),
                            _ => None,
                        };
                    }
                }
                _ => {
                    if self.eat("(") {
                        let mut depth = 1;
                        while depth > 0 {
                            if self.at_eof() {
                                return Err(self.error("Unterminated attribute"));
                            }
                            if self.eat("(") {
                                depth += 1;
                            } else if self.eat(")") {
                                depth -= 1;
                            } else {
                                self.bump();
                            }
                        }
                    }
                }
            }
        }
        self.expect(")")?;
        Ok(())
    }

    /// Evaluates a C integer constant expression.
    fn parse_const_expr(&mut self) -> Result<i64, ParseError> {
        if self.depth >= MAX_NESTING {
            return Err(self.error("constant expression nesting too deep"));
        }
        self.depth += 1;
        let result = self.parse_binary_expr(0);
        self.depth -= 1;
        result
    }

    /// Parses a precedence-climbing binary constant expression.
    fn parse_binary_expr(&mut self, min_prec: u8) -> Result<i64, ParseError> {
        let mut left = self.parse_unary_expr()?;
        loop {
            let (prec, op) = match self.peek_text() {
                "|" => (1, "|"),
                "^" => (2, "^"),
                "&" => (3, "&"),
                "<<" => (4, "<<"),
                ">>" => (4, ">>"),
                "+" => (5, "+"),
                "-" => (5, "-"),
                "*" => (6, "*"),
                "/" => (6, "/"),
                "%" => (6, "%"),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let right = self.parse_binary_expr(prec + 1)?;
            left = match op {
                "|" => left | right,
                "^" => left ^ right,
                "&" => left & right,
                "<<" => left.wrapping_shl(right as u32),
                ">>" => left.wrapping_shr(right as u32),
                "+" => left.wrapping_add(right),
                "-" => left.wrapping_sub(right),
                "*" => left.wrapping_mul(right),
                "/" => {
                    if right == 0 {
                        return Err(self.error("Division by zero"));
                    }
                    left.wrapping_div(right)
                }
                "%" => {
                    if right == 0 {
                        return Err(self.error("Division by zero"));
                    }
                    left.wrapping_rem(right)
                }
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    /// Parses a unary or primary constant expression.
    fn parse_unary_expr(&mut self) -> Result<i64, ParseError> {
        if self.eat("+") {
            return self.parse_unary_expr();
        }
        if self.eat("-") {
            return Ok(self.parse_unary_expr()?.wrapping_neg());
        }
        if self.eat("~") {
            return Ok(!self.parse_unary_expr()?);
        }
        if self.eat("!") {
            return Ok((self.parse_unary_expr()? == 0) as i64);
        }
        self.parse_primary_expr()
    }

    /// Parses a primary integer, char, or identifier constant.
    fn parse_primary_expr(&mut self) -> Result<i64, ParseError> {
        if self.eat("(") {
            let value = self.parse_const_expr()?;
            self.expect(")")?;
            return Ok(value);
        }
        let token = self.bump();
        match token.kind {
            TokenKind::Number => parse_integer_literal(&token.text)
                .ok_or_else(|| self.error_on(&token, "Invalid integer literal")),
            TokenKind::Ident => self
                .constants
                .get(&token.text)
                .copied()
                .ok_or_else(|| self.error_on(&token, &format!("Undefined constant \"{}\"", token.text))),
            _ => Err(self.error_on(&token, "Expected constant expression")),
        }
    }
}

#[derive(Clone)]
struct RawField {
    name: Option<String>,
    ty: TypeId,
    bit_width: Option<u8>,
    attrs: Attrs,
    #[allow(dead_code)]
    anonymous: bool,
}

struct NaturalLayout {
    fields: Vec<Field>,
    bits: usize,
    size: usize,
    align: usize,
}

/// Returns the identifier at the leaf of a declarator tree.
fn derived_name(derived: &Derived) -> Option<String> {
    match derived {
        Derived::Ident(name) => name.clone(),
        Derived::Pointer { to, .. } | Derived::Array { of: to, .. } | Derived::Function { of: to, .. } => {
            derived_name(to)
        }
    }
}


/// Chooses the enum storage width and signedness, honoring packing.
fn enum_layout(values: &[(String, i64)], packed: bool) -> (usize, bool) {
    let min = values.iter().map(|(_, value)| *value).min().unwrap_or(0);
    let max = values.iter().map(|(_, value)| *value).max().unwrap_or(0);
    let signed = min < 0;
    if packed {
        if signed {
            if min >= i8::MIN as i64 && max <= i8::MAX as i64 {
                return (1, true);
            }
            if min >= i16::MIN as i64 && max <= i16::MAX as i64 {
                return (2, true);
            }
            if min >= i32::MIN as i64 && max <= i32::MAX as i64 {
                return (4, true);
            }
            (8, true)
        } else {
            if max <= u8::MAX as i64 {
                return (1, false);
            }
            if max <= u16::MAX as i64 {
                return (2, false);
            }
            if max <= u32::MAX as i64 {
                return (4, false);
            }
            (8, false)
        }
    } else if signed {
        if min >= i32::MIN as i64 && max <= i32::MAX as i64 {
            (4, true)
        } else {
            (8, true)
        }
    } else if max <= u32::MAX as i64 {
        (4, false)
    } else {
        (8, false)
    }
}

/// Returns the maximum supported object alignment.
fn max_alignment() -> usize {
    16
}

/// Returns the target's `long double` size and alignment.
fn long_double_layout() -> (usize, usize) {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        (8, 8)
    } else {
        (16, 16)
    }
}

/// Parses a C decimal, hexadecimal, octal, or binary integer literal.
fn parse_integer_literal(text: &str) -> Option<i64> {
    let cleaned = text.trim_end_matches(|c: char| matches!(c, 'u' | 'U' | 'l' | 'L'));
    if let Some(hex) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().or_else(|| u64::from_str_radix(hex, 16).ok().map(|value| value as i64));
    }
    if let Some(bin) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).ok().or_else(|| u64::from_str_radix(bin, 2).ok().map(|value| value as i64));
    }
    // C's leading-zero form is octal. Invalid digits (`08`) are a parse error like php-src's
    // `<OCTNUMBER>` token, never a silently reinterpreted decimal.
    if cleaned.len() > 1 && cleaned.starts_with('0') {
        let octal = &cleaned[1..];
        return i64::from_str_radix(octal, 8).ok()
            .or_else(|| u64::from_str_radix(octal, 8).ok().map(|value| value as i64));
    }
    cleaned.parse::<i64>().ok().or_else(|| cleaned.parse::<u64>().ok().map(|value| value as i64))
}

/// Splits a header into comments-free C source, object-like `#define` constants, and
/// the `FFI_LIB`/`FFI_SCOPE` directives php-src recognizes in `FFI::load()` files.
fn preprocess(code: &str) -> (String, HashMap<String, i64>, Option<String>, Option<String>) {
    let mut defines = HashMap::new();
    let mut scope = None;
    let mut library = None;
    let mut output = String::new();
    let mut lines = code.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix("define") {
                let rest = rest.trim_start();
                let mut parts = rest.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or_default();
                let value = parts.next().unwrap_or_default().trim();
                if name == "FFI_SCOPE" {
                    scope = Some(unquote_c(value));
                } else if name == "FFI_LIB" {
                    library = Some(unquote_c(value));
                } else if !name.is_empty() && !name.contains('(') {
                    if let Some(value) = parse_integer_literal(value) {
                        defines.insert(name.to_string(), value);
                    }
                }
            }
            // Preprocessor lines are not C declarations, but they still occupy a source line:
            // keep the newline so every later token and diagnostic keeps php-src's line number.
            // Continuations are rare in FFI headers; treat each physical line independently.
            output.push('\n');
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    (output, defines, scope, library)
}

/// Decodes one C string literal into its bytes/characters.
fn unquote_c(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

/// Splits C declaration source into parser tokens and diagnostics.
fn tokenize(code: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if byte == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            tokens.push(Token {
                text: code[start..i].to_string(),
                kind: TokenKind::String,
                line,
            });
            continue;
        }
        if byte == b'.' && bytes.get(i + 1) == Some(&b'.') && bytes.get(i + 2) == Some(&b'.') {
            tokens.push(Token {
                text: "...".to_string(),
                kind: TokenKind::Punct,
                line,
            });
            i += 3;
            continue;
        }
        if byte.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'\'')
            {
                i += 1;
            }
            tokens.push(Token {
                text: code[start..i].to_string(),
                kind: TokenKind::Number,
                line,
            });
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            tokens.push(Token {
                text: code[start..i].to_string(),
                kind: TokenKind::Ident,
                line,
            });
            continue;
        }
        if (byte == b'<' && bytes.get(i + 1) == Some(&b'<'))
            || (byte == b'>' && bytes.get(i + 1) == Some(&b'>'))
        {
            tokens.push(Token {
                text: code[i..i + 2].to_string(),
                kind: TokenKind::Punct,
                line,
            });
            i += 2;
            continue;
        }
        tokens.push(Token {
            text: (byte as char).to_string(),
            kind: TokenKind::Punct,
            line,
        });
        i += 1;
    }
    tokens.push(Token {
        text: String::new(),
        kind: TokenKind::Eof,
        line,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the C declaration parser and record layout.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - Expectations mirror the php-src `ext/ffi` PHPT corpus sizes and type names.

    use super::*;

    /// Parses a declaration block into a fresh store.
    fn parse(code: &str) -> (TypeStore, ParsedContext) {
        let (mut store, names) = TypeStore::with_builtins();
        let context = parse_context(&mut store, &names, code).expect("parse");
        (store, context)
    }

    /// Parses a type name visible in a context.
    #[test]
    fn parses_struct_layouts_and_bitfields() {
        let (store, context) = parse(
            "struct { char a; uint32_t b; };\nstruct __attribute__((packed)) { char a; uint32_t b; };",
        );
        let names = context.names;
        let _ = names;
        let types: Vec<_> = (0..store.len()).map(|id| store.get(id).clone()).collect();
        let structs: Vec<_> = types
            .iter()
            .filter(|ty| matches!(ty.kind, TypeKind::Struct(_)))
            .collect();
        assert_eq!(structs[0].size, 8);
        assert_eq!(structs[0].align, 4);
        assert_eq!(structs[1].size, 5);
        assert_eq!(structs[1].align, 1);
    }

    /// An unclosed GNU attribute is a diagnostic, never an infinite token loop.
    #[test]
    fn unterminated_attributes_report_an_error() {
        let (mut store, names) = TypeStore::with_builtins();
        let result = parse_context(&mut store, &names, "struct __attribute__((");
        assert!(result.is_err(), "unterminated attribute must fail");
    }

    /// `const int *p` qualifies the pointee; `int * const q` qualifies the pointer.
    #[test]
    fn const_qualifies_the_declared_base_type_not_the_pointer() {
        let (store, _) = parse("struct { const int *p; int * const q; };");
        let record = store
            .types()
            .iter()
            .find_map(|ty| match &ty.kind {
                TypeKind::Struct(record) if record.fields.len() == 2 => Some(record),
                _ => None,
            })
            .expect("record");
        let pointer = store.get(record.fields[0].ty);
        let TypeKind::Pointer(pointee) = pointer.kind else { panic!("p is a pointer") };
        assert!(!pointer.is_const, "the pointer itself is writable");
        assert!(store.get(pointee).is_const, "the pointee is read-only");
        let pointer = store.get(record.fields[1].ty);
        assert!(pointer.is_const, "q is a read-only pointer");
        let TypeKind::Pointer(pointee) = pointer.kind else { panic!("q is a pointer") };
        assert!(!store.get(pointee).is_const, "q's pointee is writable");
    }

    /// Non-packed bit-fields follow php-src's `uint32_t` allocation-unit rule.
    ///
    /// Consecutive bit-fields share one unit even when the declared field type is narrower or
    /// wider, the record gains 4-byte alignment, and the size grows by 32-bit units. These
    /// expectations come from PHP 8.5.10 (`FFI::sizeof`/`FFI::alignof`) and its `ffi.c`
    /// `zend_ffi_add_bit_field`.
    #[test]
    fn bitfields_follow_php_src_allocation_units() {
        let cases: &[(&str, usize, usize, usize, u16)] = &[
            ("struct t { short a:12; int b:20; };", 4, 4, 0, 12),
            ("struct a1 { int a:30; short b:4; };", 8, 4, 0, 30),
            ("struct a2 { int a:30; short b:2; };", 4, 4, 0, 30),
            ("struct a3 { long a:60; int b:8; };", 12, 4, 0, 60),
            ("struct u { int a:20; int b:12; };", 4, 4, 0, 20),
            ("struct w { int a:20; int b:5; };", 4, 4, 0, 20),
        ];
        for (declaration, size, align, offset, first_bit) in cases {
            let (store, _) = parse(declaration);
            let record = store
                .types()
                .iter()
                .find_map(|ty| match &ty.kind {
                    TypeKind::Struct(record) if record.fields.len() == 2 => Some(record),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("record for {declaration}"));
            let ty = store
                .types()
                .iter()
                .find(|ty| matches!(&ty.kind, TypeKind::Struct(candidate) if std::ptr::eq(candidate, record)))
                .expect("struct type");
            assert_eq!((ty.size, ty.align), (*size, *align), "{declaration}");
            let field = &record.fields[1];
            assert_eq!((field.offset, field.bit_offset), (*offset, *first_bit), "{declaration}");
        }
    }

    /// Packed bit-fields consume contiguous bits, as PHP's FFI 032 fixture requires.
    #[test]
    fn packed_bitfields_are_contiguous() {
        let (store, _) = parse(
            "struct __attribute__((packed)) { int a:2; unsigned long long b:64; int c:3; unsigned int d:3; };",
        );
        let record = store
            .types()
            .iter()
            .find_map(|ty| match &ty.kind {
                TypeKind::Struct(record) if !record.fields.is_empty() => Some(record),
                _ => None,
            })
            .expect("record");
        assert_eq!(record.fields.len(), 4);
        assert_eq!(store.get(record.fields[3].ty).size, 4);
        assert_eq!(record.fields[0].offset, 0);
    }

    /// A tagged struct resolves its self-referential pointer to the same completed type.
    #[test]
    fn recursive_structs_resolve_to_the_outer_tag() {
        let (store, _) = parse("struct Node { int value; struct Node *next; };");
        let (id, record) = store
            .types()
            .iter()
            .enumerate()
            .find_map(|(id, ty)| match &ty.kind {
                TypeKind::Struct(record) if record.fields.len() == 2 => Some((id, record)),
                _ => None,
            })
            .expect("record");
        let TypeKind::Pointer(inner) = store.get(record.fields[1].ty).kind else {
            panic!("next is a pointer");
        };
        assert_eq!(inner, id, "next points at the completed outer tag");
    }

    /// Deep declarator nesting is diagnosed before the Rust stack overflows.
    #[test]
    fn deep_declarators_report_a_nesting_diagnostic() {
        let (mut store, names) = TypeStore::with_builtins();
        let source = format!("typedef int {}x{};", "(".repeat(2_000), ")".repeat(2_000));
        let error = parse_context(&mut store, &names, &source).expect_err("deep nesting");
        assert!(
            error.message.contains("nesting too deep"),
            "{}",
            error.message
        );
        // php-src accepts 1_000 nested declarator parentheses; keep that working below the cap.
        let (mut store, names) = TypeStore::with_builtins();
        let accepted = format!("typedef int {}x{};", "(".repeat(1_000), ")".repeat(1_000));
        parse_context(&mut store, &names, &accepted).expect("1000 levels parse");
    }

    /// A bare `char` field follows the target ABI's signedness, like the builtin type.
    #[test]
    fn bare_char_signedness_matches_target_abi() {
        let (store, _) = parse("struct { char c; };\n");
        let record = store
            .types()
            .iter()
            .find_map(|ty| match &ty.kind {
                TypeKind::Struct(record) if record.fields.len() == 1 => Some(record),
                _ => None,
            })
            .expect("record");
        let field = store.get(record.fields[0].ty);
        assert!(matches!(
            field.kind,
            TypeKind::Char { signed } if signed == crate::types::char_is_signed()
        ));
    }

    /// C integer literals honor octal radix and reject invalid octal digits.
    #[test]
    fn integer_literals_follow_c_radices() {
        assert_eq!(parse_integer_literal("0644"), Some(0o644));
        assert_eq!(parse_integer_literal("0777"), Some(0o777));
        assert_eq!(parse_integer_literal("0"), Some(0));
        assert_eq!(parse_integer_literal("42"), Some(42));
        assert_eq!(parse_integer_literal("0x1f"), Some(31));
        assert_eq!(parse_integer_literal("0b101"), Some(5));
        assert_eq!(parse_integer_literal("08"), None);
    }

    /// Invalid GNU alignment values fail like php-src instead of silently becoming one byte.
    #[test]
    fn invalid_alignment_values_report_php_src_diagnostics() {
        for declaration in [
            "struct __attribute__((aligned(3))) { int a; };",
            "struct __attribute__((aligned(0))) { int a; };",
            "struct __attribute__((aligned(foo))) { int a; };",
        ] {
            let (mut store, names) = TypeStore::with_builtins();
            let error = parse_context(&mut store, &names, declaration).expect_err(declaration);
            assert_eq!(error.message, "Incorrect \"alignment\" value", "{declaration}");
        }
        let (mut store, names) = TypeStore::with_builtins();
        let error = parse_context(&mut store, &names, "struct __attribute__((aligned())) { int a; };")
            .expect_err("empty alignment argument");
        assert_eq!(error.message, "unexpected ')'");
    }

    /// Preprocessor directives occupy their source line so later diagnostics keep php-src's line.
    #[test]
    fn preprocessor_lines_preserve_diagnostic_lines() {
        let (mut store, names) = TypeStore::with_builtins();
        let error = parse_context(&mut store, &names, "#define A 1\n#define B 2\nint broken(];")
            .expect_err("malformed declaration");
        assert_eq!(error.line, 3);
    }

    /// Packed enum widths follow the smallest signed/unsigned storage that fits.
    #[test]
    fn packed_enum_widths() {
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0xff)], true), (1, false));
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0x100)], true), (2, false));
        assert_eq!(enum_layout(&[("a".into(), -1), ("b".into(), 0x7f)], true), (1, true));
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0x100000000)], true), (8, false));
    }
}
