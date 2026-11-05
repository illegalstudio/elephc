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

use std::collections::HashMap;

use crate::types::{
    align_up, CType, Field, FuncType, StructType, TagKeyword, TypeId, TypeKind, TypeStore,
    ATTR_GCC_STRUCT, ATTR_INCOMPLETE_TAG, ATTR_MS_STRUCT, ATTR_PACKED,
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

struct Parser<'a> {
    store: &'a mut TypeStore,
    names: HashMap<String, TypeId>,
    user_typedefs: std::collections::HashSet<String>,
    tags: HashMap<String, TypeId>,
    constants: HashMap<String, i64>,
    tokens: Vec<Token>,
    pos: usize,
    scope: Option<String>,
    library: Option<String>,
    functions: HashMap<String, TypeId>,
    globals: HashMap<String, TypeId>,
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
        scope,
        library,
        functions: HashMap::new(),
        globals: HashMap::new(),
        allow_star_array: false,
    };
    parser.parse_declarations()?;
    Ok(ParsedContext {
        names: parser.names,
        tags: parser.tags,
        functions: parser.functions,
        globals: parser.globals,
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
        scope: None,
        library: None,
        functions: HashMap::new(),
        globals: HashMap::new(),
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

    fn error_on(&self, token: &Token, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            line: token.line,
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_text(&self) -> &str {
        &self.peek().text
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn eat(&mut self, text: &str) -> bool {
        if self.peek_text() == text {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, text: &str) -> Result<Token, ParseError> {
        if self.peek_text() == text {
            Ok(self.bump())
        } else {
            Err(self.error(&format!("Expected \"{}\"", text)))
        }
    }

    fn eat_ident(&mut self) -> Option<String> {
        if self.peek().kind == TokenKind::Ident {
            Some(self.bump().text)
        } else {
            None
        }
    }

    fn parse_declarations(&mut self) -> Result<(), ParseError> {
        while !self.at_eof() {
            if self.eat(";") {
                continue;
            }
            self.parse_declaration()?;
        }
        Ok(())
    }

    fn parse_declaration(&mut self) -> Result<(), ParseError> {
        let mut attrs = Attrs::default();
        let base = self.parse_specifiers(&mut attrs)?;
        if self.eat(";") {
            return Ok(());
        }
        loop {
            let derived = self.parse_declarator()?;
            let field_attrs = self.parse_trailing_attrs()?;
            let mut ty = self.apply(base, &derived)?;
            ty = self.apply_type_attrs(ty, &attrs)?;
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
                self.functions.insert(name, ty);
            } else {
                self.ensure_complete_value_type(ty)?;
                if let Some(name) = name {
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

    fn parse_type_name_inner(&mut self) -> Result<TypeId, ParseError> {
        let mut attrs = Attrs::default();
        let base = self.parse_specifiers(&mut attrs)?;
        let base = self.apply_type_attrs(base, &attrs)?;
        if self.at_eof() {
            return Ok(base);
        }
        let derived = self.parse_declarator()?;
        self.apply(base, &derived)
    }

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
                "extern" | "static" | "auto" | "register" | "inline" | "_Noreturn" | "__extension__"
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
            "char" => self.store.push(CType {
                kind: TypeKind::Char { signed: signed.unwrap_or(true) },
                size: 1,
                align: 1,
                attrs: 0,
                display: if signed == Some(false) { "uint8_t".to_string() } else if signed == Some(true) { "int8_t".to_string() } else { "char".to_string() },
                is_const: false,
            }),
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
                self.parse_struct_body(id, keyword == TagKeyword::Union, attrs, tag.clone())?;
                if let Some(key) = key {
                    self.tags.insert(key, id);
                }
                Ok(id)
            }
            TagKeyword::Enum => {
                let id = existing.unwrap_or_else(|| self.allocate_incomplete(keyword, tag.as_deref()));
                self.parse_enum_body(id, attrs, tag.clone())?;
                if let Some(key) = key {
                    self.tags.insert(key, id);
                }
                Ok(id)
            }
        }
    }

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
                ty = self.apply_type_attrs(ty, &field_attrs)?;
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

    fn layout_record(
        &mut self,
        raw: &[RawField],
        union: bool,
        packed: bool,
        current_tag: Option<&str>,
    ) -> Result<NaturalLayout, ParseError> {
        let mut fields = Vec::new();
        let mut bit_cursor = 0usize;
        let mut byte_size = 0usize;
        let mut align = 1usize;
        let bit_unit_bytes = raw
            .iter()
            .filter(|field| field.bit_width.is_some())
            .map(|field| self.store.get(field.ty).size.max(1))
            .max()
            .unwrap_or(1);
        let bit_unit_align = raw
            .iter()
            .filter(|field| field.bit_width.is_some())
            .map(|field| self.store.get(field.ty).align.max(1))
            .max()
            .unwrap_or(1);
        let mut open_unit: Option<(usize, usize)> = None;

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
                if width > ty.size * 8 {
                    return Err(self.error("Invalid bit-field width"));
                }
                if union {
                    byte_size = byte_size.max(if packed { width.div_ceil(8) } else { ty.size });
                    align = align.max(if packed { 1 } else { ty.align.max(1) });
                    fields.push(Field {
                        name: field.name.clone(),
                        ty: field.ty,
                        offset: 0,
                        bit_offset: 0,
                        bit_width: field.bit_width,
                        readonly,
                    });
                } else if field_packed || packed {
                    let absolute = bit_cursor;
                    bit_cursor += width;
                    align = align.max(1);
                    fields.push(Field {
                        name: field.name.clone(),
                        ty: field.ty,
                        offset: absolute / 8,
                        bit_offset: (absolute % 8) as u8,
                        bit_width: field.bit_width,
                        readonly,
                    });
                } else {
                    let unit_bits = bit_unit_bytes * 8;
                    let restart = open_unit
                        .map(|(_, used)| used + width > unit_bits)
                        .unwrap_or(true);
                    let (offset, used) = if restart {
                        let offset = align_up(bit_cursor.div_ceil(8), bit_unit_align);
                        let used = 0;
                        open_unit = Some((offset, width));
                        bit_cursor = bit_cursor.max(offset * 8 + width);
                        (offset, used)
                    } else {
                        let (offset, used) = open_unit.unwrap();
                        open_unit = Some((offset, used + width));
                        bit_cursor = bit_cursor.max(offset * 8 + used + width);
                        (offset, used)
                    };
                    align = align.max(bit_unit_align);
                    fields.push(Field {
                        name: field.name.clone(),
                        ty: field.ty,
                        offset,
                        bit_offset: used as u8,
                        bit_width: field.bit_width,
                        readonly,
                    });
                }
            } else {
                open_unit = None;
                let offset = if union {
                    0
                } else {
                    align_up(bit_cursor.div_ceil(8), field_align)
                };
                if union {
                    byte_size = byte_size.max(ty.size);
                } else {
                    bit_cursor = offset * 8 + ty.size * 8;
                }
                align = align.max(field_align);
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
        let bits = if union { byte_size * 8 } else { bit_cursor };
        Ok(NaturalLayout {
            fields,
            bits,
            size: align_up(bits.div_ceil(8), align),
            align,
        })
    }

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

    fn parse_declarator(&mut self) -> Result<Derived, ParseError> {
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
            if self.peek_text() == ")" || self.peek_text() == "," {
                let ty = self.apply_type_attrs(base, &attrs)?;
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
            let ty = self.apply_type_attrs(ty, &attrs)?;
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

    fn apply(&mut self, base: TypeId, derived: &Derived) -> Result<TypeId, ParseError> {
        match derived {
            Derived::Ident(_) => Ok(base),
            Derived::Pointer { to, is_const } => {
                let pointer = self.store.pointer(base);
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
                let array = self.store.array(base, *len);
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

    fn apply_mode(&mut self, ty: TypeId, mode: Option<Mode>) -> Result<TypeId, ParseError> {
        let Some(mode) = mode else { return Ok(ty) };
        let new = match mode {
            Mode::Qi => self.store.int(8, self.store.get(ty).kind_code() == crate::types::TYPE_SINT8, "int8_t"),
            Mode::Hi => self.store.int(16, true, "int16_t"),
            Mode::Si => self.store.int(32, true, "int32_t"),
            Mode::Di => self.store.int(64, true, "int64_t"),
            Mode::Sf => self.store.float(32, "float", 4, 4),
            Mode::Df => self.store.float(64, "double", 8, 8),
        };
        Ok(new)
    }

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

    fn parse_attribute(&mut self, attrs: &mut Attrs) -> Result<(), ParseError> {
        if self.eat("__declspec") {
            self.expect("(")?;
            while !self.eat(")") {
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
                        let value = self.parse_const_expr().unwrap_or(0);
                        self.expect(")")?;
                        attrs.aligned = Some(Some(value.max(1) as usize));
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

    fn parse_const_expr(&mut self) -> Result<i64, ParseError> {
        self.parse_binary_expr(0)
    }

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

fn derived_name(derived: &Derived) -> Option<String> {
    match derived {
        Derived::Ident(name) => name.clone(),
        Derived::Pointer { to, .. } | Derived::Array { of: to, .. } | Derived::Function { of: to, .. } => {
            derived_name(to)
        }
    }
}


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

fn max_alignment() -> usize {
    16
}

fn long_double_layout() -> (usize, usize) {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        (8, 8)
    } else {
        (16, 16)
    }
}

fn parse_integer_literal(text: &str) -> Option<i64> {
    let cleaned = text.trim_end_matches(|c: char| matches!(c, 'u' | 'U' | 'l' | 'L'));
    if let Some(hex) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().or_else(|| u64::from_str_radix(hex, 16).ok().map(|value| value as i64));
    }
    if let Some(bin) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).ok().or_else(|| u64::from_str_radix(bin, 2).ok().map(|value| value as i64));
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
            // Preprocessor lines are not C declarations. Continuations are rare in FFI
            // headers; treat each physical line independently.
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    (output, defines, scope, library)
}

fn unquote_c(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

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

    /// Packed enum widths follow the smallest signed/unsigned storage that fits.
    #[test]
    fn packed_enum_widths() {
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0xff)], true), (1, false));
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0x100)], true), (2, false));
        assert_eq!(enum_layout(&[("a".into(), -1), ("b".into(), 0x7f)], true), (1, true));
        assert_eq!(enum_layout(&[("a".into(), 0), ("b".into(), 0x100000000)], true), (8, false));
    }
}
