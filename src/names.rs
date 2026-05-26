//! Purpose:
//! Defines canonical PHP name handling and generated symbol naming helpers.
//! Centralizes fully-qualified names, case-folded lookup keys, and assembly-safe mangling.
//!
//! Called from:
//! - `crate::parser`, `crate::resolver`, `crate::name_resolver`, and codegen metadata passes.
//!
//! Key details:
//! - PHP symbol lookup and emitted assembly labels depend on these transformations staying stable.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Kind of PHP name based on how it was written in source.
///
/// - `Unqualified`: single identifier with no namespace separator (e.g., `Foo`)
/// - `Qualified`: contains a namespace separator but is not root-anchored (e.g., `Namespace\Foo`)
/// - `FullyQualified`: begins with a root separator (e.g., `\Namespace\Foo`)
pub enum NameKind {
    Unqualified,
    Qualified,
    FullyQualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// PHP name with resolution context stored alongside its text parts.
///
/// `parts` holds the raw identifier segments (e.g., `["Namespace", "Foo"]`).
/// `text` holds the canonical backslash-joined form used for lookup and symbol emission.
pub struct Name {
    pub kind: NameKind,
    pub parts: Vec<String>,
    text: String,
}

impl Name {
    /// Constructs an unqualified name from a single identifier.
    ///
    /// Sets `kind` to `NameKind::Unqualified` and `parts` to a single-element vector.
    pub fn unqualified(name: impl Into<String>) -> Self {
        Self {
            kind: NameKind::Unqualified,
            parts: vec![name.into()],
            text: String::new(),
        }
        .with_text()
    }

    /// Constructs a name from a list of namespace parts.
    ///
    /// Infers `NameKind::Unqualified` when `parts.len() <= 1`, otherwise `NameKind::Qualified`.
    pub fn qualified(parts: Vec<String>) -> Self {
        let kind = if parts.len() <= 1 {
            NameKind::Unqualified
        } else {
            NameKind::Qualified
        };
        Self {
            kind,
            parts,
            text: String::new(),
        }
        .with_text()
    }

    /// Constructs a name from explicit kind and parts.
    ///
    /// Downgrades `NameKind::Qualified` to `NameKind::Unqualified` when `parts.len() <= 1`.
    pub fn from_parts(kind: NameKind, parts: Vec<String>) -> Self {
        let kind = if parts.len() <= 1 && kind == NameKind::Qualified {
            NameKind::Unqualified
        } else {
            kind
        };
        Self {
            kind,
            parts,
            text: String::new(),
        }
        .with_text()
    }

    /// Builds the canonical text representation by joining parts with backslashes.
    ///
    /// Called internally after construction to populate `self.text` from `self.parts`.
    fn with_text(mut self) -> Self {
        self.text = self.parts.join("\\");
        self
    }

    /// Returns the canonical backslash-joined text representation.
    ///
    /// Result matches the string used for `php_symbol_key` and symbol emission.
    pub fn as_canonical(&self) -> String {
        self.text.clone()
    }

    /// Returns a borrowed slice of the canonical text representation.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns `true` if the name is `NameKind::Unqualified`.
    pub fn is_unqualified(&self) -> bool {
        self.kind == NameKind::Unqualified
    }

    /// Returns `true` if the name is `NameKind::FullyQualified` (begins with `\`).
    pub fn is_fully_qualified(&self) -> bool {
        self.kind == NameKind::FullyQualified
    }

    /// Returns the final identifier segment, or `None` if `parts` is empty.
    ///
    /// For `FullyQualified` names this is the short name without any namespace prefix.
    pub fn last_segment(&self) -> Option<&str> {
        self.parts.last().map(String::as_str)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::ops::Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Name::unqualified(value)
    }
}

impl From<String> for Name {
    fn from(value: String) -> Self {
        Name::unqualified(value)
    }
}

/// Returns the canonical declaration name for a namespaced symbol.
///
/// If `namespace` is provided and non-empty, returns `"namespace\local_name"`;
/// otherwise returns just `local_name`. Used for matching declarations to their
/// canonical PHP symbol keys.
pub fn canonical_name_for_decl(namespace: Option<&str>, local_name: &str) -> String {
    if let Some(namespace) = namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, local_name);
        }
    }
    local_name.to_string()
}

/// Returns the lowercase ASCII key used for PHP symbol lookup.
///
/// PHP symbol lookups are case-insensitive; this produces the normalized key
/// for `php_symbol_key` lookups against the symbol table.
pub fn php_symbol_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Returns an assembly-safe mangled form of a fully-qualified name.
///
/// Replaces non-alphanumeric characters: `_` → `_u_`, `\` → `_N_`. Alphanumeric
/// characters and digits are preserved. Panics on unsupported characters.
/// Used to produce valid assembly labels from PHP FQNs.
pub fn mangle_fqn(name: &str) -> String {
    let mut mangled = String::new();
    for ch in name.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => mangled.push(ch),
            '_' => mangled.push_str("_u_"),
            '\\' => mangled.push_str("_N_"),
            _ => panic!("unsupported symbol character in mangled name: {}", ch),
        }
    }
    mangled
}

/// Returns the global function symbol label for a given PHP function name.
///
/// Format: `_fn_<mangled_fqn>`. Used for user-defined function entry points.
pub fn function_symbol(name: &str) -> String {
    format!("_fn_{}", mangle_fqn(name))
}

/// Returns the variant-active dispatch helper symbol for a given PHP function name.
///
/// Format: `_fn_variant_active_<mangled_fqn>`. Used for functions with conditional
/// compilation branches that need runtime variant selection.
pub fn function_variant_active_symbol(name: &str) -> String {
    format!("_fn_variant_active_{}", mangle_fqn(name))
}

/// Returns the function epilogue symbol for a given PHP function name.
///
/// Format: `_fn_<mangled_fqn>_epilogue`. Appends `_epilogue` to `function_symbol`.
pub fn function_epilogue_symbol(name: &str) -> String {
    format!("{}_epilogue", function_symbol(name))
}

/// Returns the instance method symbol for a class/method pair.
///
/// Format: `_method_<mangled_class>_<mangled_method>`. Used for virtual dispatch
/// and method table entries.
pub fn method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "_method_{}_{}",
        mangle_fqn(class_name),
        mangle_fqn(method_name)
    )
}

/// Returns the interface method wrapper symbol for a class/interface/method triplet.
///
/// Format: `_ifacewrap_<class_id>_<interface_id>_<mangled_method>`. Used by the
/// runtime to route interface method calls through concrete implementation wrappers.
pub fn interface_method_wrapper_symbol(
    class_id: u64,
    interface_id: u64,
    method_name: &str,
) -> String {
    format!(
        "_ifacewrap_{}_{}_{}",
        class_id,
        interface_id,
        mangle_fqn(method_name)
    )
}

/// Returns the static method symbol for a class/method pair.
///
/// Format: `_static_<mangled_class>_<mangled_method>`. Used for static method
/// dispatch and method table entries.
pub fn static_method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "_static_{}_{}",
        mangle_fqn(class_name),
        mangle_fqn(method_name)
    )
}

/// Returns the static property symbol for a class/property pair.
///
/// Format: `_static_prop_<mangled_class>_<mangled_property>`. Used for static
/// property access and the property lookup table.
pub fn static_property_symbol(class_name: &str, property_name: &str) -> String {
    format!(
        "_static_prop_{}_{}",
        mangle_fqn(class_name),
        mangle_fqn(property_name)
    )
}

/// Returns the enum case symbol for an enum/case pair.
///
/// Format: `_enum_case_<mangled_enum>_<mangled_case>`. Used for enum case
/// lookup and the enum case table.
pub fn enum_case_symbol(enum_name: &str, case_name: &str) -> String {
    format!(
        "_enum_case_{}_{}",
        mangle_fqn(enum_name),
        mangle_fqn(case_name)
    )
}
