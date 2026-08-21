//! Purpose:
//! Defines canonical PHP name handling and generated symbol naming helpers.
//! Centralizes fully-qualified names, case-folded lookup keys, and assembly-safe mangling.
//!
//! Called from:
//! - `crate::parser`, `crate::resolver`, `crate::name_resolver`, and codegen metadata passes.
//!
//! Key details:
//! - PHP symbol lookup and emitted assembly labels depend on these transformations staying stable.
//! - Composite symbols (class + member, function + static local, …) must be built with
//!   `join_symbol_fragments()`/`join_php_symbol()`. Joining mangled fragments with a bare `_`
//!   is ambiguous because mangled fragments contain `_`, which silently merged unrelated PHP
//!   declarations onto one storage cell.

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
    /// Formats this value for display or debug output.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::ops::Deref for Name {
    type Target = str;

    /// Returns the borrowed target for deref coercions.
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for Name {
    /// Compares this value with another value for equality.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Name {
    /// Compares this value with another value for equality.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Name {
    /// Compares this value with another value for equality.
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl From<&str> for Name {
    /// Converts the input value into this type.
    fn from(value: &str) -> Self {
        Name::unqualified(value)
    }
}

impl From<String> for Name {
    /// Converts the input value into this type.
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
/// ASCII letters and digits are preserved; `_` → `_u_` and `\` → `_N_`. Any other
/// character (including the non-ASCII bytes PHP permits in identifiers) is escaped as
/// one `_xNN_` group per UTF-8 byte. The `_x` prefix cannot collide with the `_u_`/`_N_`
/// escapes, so the mapping stays injective. Total by construction — never panics — so
/// an unusual symbol name can never crash the compiler.
pub fn mangle_fqn(name: &str) -> String {
    let mut mangled = String::new();
    for ch in name.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => mangled.push(ch),
            '_' => mangled.push_str("_u_"),
            '\\' => mangled.push_str("_N_"),
            other => {
                let mut buf = [0u8; 4];
                for byte in other.encode_utf8(&mut buf).bytes() {
                    mangled.push_str(&format!("_x{:02x}_", byte));
                }
            }
        }
    }
    mangled
}

/// Separator inserted between a symbol prefix and mangled fragments when at least one
/// fragment carries a `mangle_fqn()` escape.
///
/// A `mangle_fqn()` result is a concatenation of single alphanumerics and the escape groups
/// `_u_`, `_N_` and `_xNN_`. Every escape opens and closes with exactly one `_` and has a
/// non-empty body, so the longest run of consecutive underscores a mangled fragment can
/// contain is two (the closing `_` of one escape followed by the opening `_` of the next),
/// and a mangled fragment can neither start nor end with `__`. Three underscores therefore
/// never occur inside a mangled fragment, which makes them usable as a boundary marker.
const ESCAPED_FRAGMENT_SEPARATOR: &str = "___";

/// Separator inserted between a symbol prefix and mangled fragments when every fragment is a
/// plain alphanumeric run. Keeps the common `_method_Foo_bar` symbol shape readable.
const COMPACT_FRAGMENT_SEPARATOR: &str = "_";

/// Returns `true` when a mangled fragment is a non-empty run of ASCII alphanumerics.
///
/// Such fragments contain no `_` at all, so a single-underscore separator between them is
/// unambiguous. Any fragment that went through a `mangle_fqn()` escape fails this test.
fn is_compact_fragment(fragment: &str) -> bool {
    !fragment.is_empty() && fragment.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Joins a fixed symbol prefix with already-mangled fragments so that the result is injective
/// in the fragment tuple.
///
/// `prefix` must be a compile-time literal without a trailing separator (e.g. `"_method"`);
/// `fragments` must be `mangle_fqn()` results or decimal numbers. Two separator regimes are
/// used and they can never be confused with one another:
///
/// - every fragment alphanumeric → `prefix_f1_f2`. The joined tail then contains exactly one
///   `_` per fragment and no run of two, so splitting on `_` recovers the tuple.
/// - otherwise → `prefix___f1___f2`. Fragments contain no `___`, so the runs of three or more
///   underscores mark exactly the boundaries. A boundary run has length 3 to 5 (a fragment
///   contributes at most one adjacent `_`), and the split is still unique because no
///   `mangle_fqn()` result stays valid when a trailing `_` is added or removed: an escape's
///   closing `_` cannot be dropped and a dangling `_` cannot be appended.
///
/// The two regimes are distinguished by the run length alone (the compact form never contains
/// two adjacent underscores in the joined tail, the escaped form always contains at least three).
pub fn join_symbol_fragments(prefix: &str, fragments: &[&str]) -> String {
    let separator = if fragments.iter().all(|fragment| is_compact_fragment(fragment)) {
        COMPACT_FRAGMENT_SEPARATOR
    } else {
        ESCAPED_FRAGMENT_SEPARATOR
    };
    let mut symbol = String::from(prefix);
    for fragment in fragments {
        symbol.push_str(separator);
        symbol.push_str(fragment);
    }
    symbol
}

/// Mangles each raw PHP name and joins them onto `prefix` with `join_symbol_fragments()`.
///
/// This is the only supported way to build a symbol or label out of more than one PHP name.
pub fn join_php_symbol(prefix: &str, names: &[&str]) -> String {
    let mangled: Vec<String> = names.iter().map(|name| mangle_fqn(name)).collect();
    let fragments: Vec<&str> = mangled.iter().map(String::as_str).collect();
    join_symbol_fragments(prefix, &fragments)
}

/// Converts an arbitrary PHP-derived name into a decorative assembly-label fragment.
///
/// Every non-alphanumeric byte collapses to `_`, so this is deliberately **not** injective:
/// `a_b` and `aéb` produce the same fragment. It may only be used for the human-readable part
/// of a label whose uniqueness is already guaranteed by a separate unique numeric id (see
/// `crate::codegen::context::FunctionContext::next_label()`). Any label that must be unique on
/// its own has to be built with `join_php_symbol()` instead.
pub fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod mangle_tests {
    use super::*;

    /// Verifies the existing `_u_`/`_N_` escapes for underscore and namespace separator
    /// are preserved unchanged by the mangling.
    #[test]
    fn mangle_fqn_preserves_existing_escapes() {
        assert_eq!(mangle_fqn("foo"), "foo");
        assert_eq!(mangle_fqn("foo_bar"), "foo_u_bar");
        assert_eq!(mangle_fqn("A\\B"), "A_N_B");
    }

    /// Verifies a non-ASCII identifier (legal in PHP) mangles into a valid assembly label
    /// instead of panicking, so unsupported characters can never crash the compiler.
    #[test]
    fn mangle_fqn_escapes_non_ascii_without_panicking() {
        let mangled = mangle_fqn("价格");
        assert!(
            mangled.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "mangled name must be an assembler-safe label, got {mangled}"
        );
    }

    /// Verifies distinct names mangle to distinct labels, so escaping does not collapse
    /// different symbols onto the same assembly label.
    #[test]
    fn mangle_fqn_distinguishes_distinct_names() {
        assert_ne!(mangle_fqn("价"), mangle_fqn("格"));
        assert_ne!(mangle_fqn("价"), mangle_fqn("a"));
        assert_ne!(mangle_fqn("a_b"), mangle_fqn("a\\b"));
    }

    /// Verifies the documented separator invariant the joiner relies on: no `mangle_fqn()`
    /// result ever contains three consecutive underscores, and none starts or ends with two.
    #[test]
    fn mangled_fragments_never_contain_the_escaped_separator() {
        let adversarial = [
            "_", "__", "___", "____", "_S_", "_u_", "_N_", "_x5f_", "\\", "\\\\", "_\\_",
            "a_\\_b", "__construct", "价_格", "_é_", "\\_\\_",
        ];
        for name in adversarial {
            let mangled = mangle_fqn(name);
            assert!(
                !mangled.contains(ESCAPED_FRAGMENT_SEPARATOR),
                "mangle_fqn({name:?}) = {mangled:?} contains the fragment separator"
            );
            assert!(
                !mangled.starts_with("__") && !mangled.ends_with("__"),
                "mangle_fqn({name:?}) = {mangled:?} must not start or end with two underscores"
            );
        }
    }

    /// Verifies the adversarial `_S_` case called out for naive separator schemes: the PHP name
    /// `_S_` mangles to a string containing `_S_`, so `_S_` would be an unusable separator,
    /// while the `___` separator survives it.
    #[test]
    fn joiner_survives_the_self_referential_separator_name() {
        assert!(mangle_fqn("_S_").contains("_S_"));
        assert_ne!(
            join_php_symbol("_p", &["a", "_S_b"]),
            join_php_symbol("_p", &["a_S", "b"])
        );
        assert_ne!(
            join_php_symbol("_p", &["a", "___b"]),
            join_php_symbol("_p", &["a___", "b"])
        );
    }

    /// Verifies the compact regime is used only for alphanumeric fragments and keeps the
    /// historical readable symbol shape.
    #[test]
    fn joiner_keeps_alphanumeric_symbols_compact() {
        assert_eq!(method_symbol("Exception", "run"), "_method_Exception_run");
        assert_eq!(static_method_symbol("Foo", "bar"), "_static_Foo_bar");
        assert_eq!(static_property_symbol("Foo", "bar"), "_static_prop_Foo_bar");
        assert_eq!(enum_case_symbol("Suit", "Hearts"), "_enum_case_Suit_Hearts");
        assert_eq!(static_local_symbol("f", "x"), "_static_local_f_x");
        assert_eq!(interface_method_wrapper_symbol(1, 2, "run"), "_ifacewrap_1_2_run");
    }

    /// Verifies the reported static-property collision (`a::$u_b` versus `a_u::$b`) now maps to
    /// two distinct `.comm` symbols instead of merging both classes onto one storage cell.
    #[test]
    fn static_property_symbols_do_not_collide_on_underscore_boundaries() {
        assert_ne!(
            static_property_symbol("a", "u_b"),
            static_property_symbol("a_u", "b")
        );
    }

    /// Verifies the reported method / static-method / enum-case collisions, which previously
    /// made valid PHP fail to assemble with a duplicate-symbol error.
    #[test]
    fn member_symbols_do_not_collide_on_underscore_boundaries() {
        assert_ne!(method_symbol("a", "u_b"), method_symbol("a_u", "b"));
        assert_ne!(
            static_method_symbol("a", "u_b"),
            static_method_symbol("a_u", "b")
        );
        assert_ne!(enum_case_symbol("a", "u_b"), enum_case_symbol("a_u", "b"));
        assert_ne!(
            interface_method_wrapper_symbol(1, 2, "u_b"),
            interface_method_wrapper_symbol(1, 2, "b")
        );
    }

    /// Verifies the static-local storage and initialization-flag namespaces stay disjoint, so a
    /// PHP static named `$x_init` can no longer alias the init flag of static `$x`.
    #[test]
    fn static_local_flag_symbols_cannot_be_spelled_by_a_php_variable() {
        assert_ne!(
            static_local_symbol("f", "x_init"),
            static_local_init_symbol("f", "x")
        );
        assert_ne!(
            static_local_symbol("f", "x"),
            static_local_init_symbol("f", "x")
        );
        assert_ne!(
            static_local_init_symbol("f", "x_init"),
            static_local_init_symbol("f_init", "x")
        );
    }

    /// Verifies static locals of two distinct functions never share one storage cell, including
    /// the non-ASCII case where the old fragment helper collapsed `é` to `_`.
    #[test]
    fn static_local_symbols_do_not_collide_across_functions() {
        assert_ne!(
            static_local_symbol("a", "b_c"),
            static_local_symbol("aéb", "c")
        );
        assert_ne!(
            static_local_symbol("a_b", "c"),
            static_local_symbol("a", "b_c")
        );
        assert_ne!(
            static_local_symbol("A::m", "x"),
            static_local_symbol("A", "m_x")
        );
    }

    /// Verifies the distinct symbol kinds cannot spell one another, in particular the
    /// static-method / static-local overlap that shared the `_static_` prefix and produced an
    /// `invalid symbol redefinition` on a class and function with the same name.
    #[test]
    fn symbol_kinds_stay_in_disjoint_namespaces() {
        assert_ne!(static_method_symbol("A", "m"), static_local_symbol("A", "m"));
        assert_ne!(
            static_method_symbol("prop", "x"),
            static_property_symbol("prop", "x")
        );
        assert_ne!(
            static_method_symbol("local", "x"),
            static_local_symbol("local", "x")
        );
        assert_ne!(
            format!("{}_epilogue", method_symbol("A", "m")),
            method_symbol("A", "m_epilogue")
        );
        assert_ne!(
            format!("{}__genbody", method_symbol("A", "m")),
            method_symbol("A", "m__genbody")
        );
    }

    /// Verifies the joiner is injective over an exhaustive cross product of adversarial name
    /// pairs, which is the property every composite symbol builder depends on.
    #[test]
    fn joiner_is_injective_over_adversarial_name_pairs() {
        let names = [
            "a", "b", "a_", "_a", "a_b", "a__b", "a_u", "u_b", "_", "__", "___", "_S_", "_u_",
            "_N_", "A\\b", "a\\b", "aéb", "a_é", "é", "x_init", "init", "prop", "local", "1",
            "12", "价格", "a价", "_x5f_", "\\_",
        ];
        let mut seen: std::collections::HashMap<String, (&str, &str)> =
            std::collections::HashMap::new();
        for left in names {
            for right in names {
                let symbol = join_php_symbol("_k", &[left, right]);
                if let Some(previous) = seen.insert(symbol.clone(), (left, right)) {
                    panic!("{previous:?} and {:?} both produce {symbol:?}", (left, right));
                }
            }
        }
    }

    /// Verifies three-fragment joins stay injective too, covering interface wrappers and the
    /// eval-bridge class/declaring-class/member labels.
    #[test]
    fn joiner_is_injective_over_adversarial_name_triples() {
        let names = ["a", "b", "a_b", "_", "_u_", "aéb", "a\\b", "1", "12"];
        let mut seen: std::collections::HashMap<String, (&str, &str, &str)> =
            std::collections::HashMap::new();
        for first in names {
            for second in names {
                for third in names {
                    let symbol = join_php_symbol("_k", &[first, second, third]);
                    if let Some(previous) = seen.insert(symbol.clone(), (first, second, third)) {
                        panic!(
                            "{previous:?} and {:?} both produce {symbol:?}",
                            (first, second, third)
                        );
                    }
                }
            }
        }
    }

    /// Verifies `label_fragment()` stays a pure decoration helper: it is documented as
    /// non-injective, and this pins the collision so nobody grows a uniqueness assumption on it.
    #[test]
    fn label_fragment_is_decorative_and_not_injective() {
        assert_eq!(label_fragment("a_b"), "a_b");
        assert_eq!(label_fragment("aéb"), label_fragment("a_b"));
    }
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

/// Returns the EIR global storage symbol for a PHP global name.
///
/// Format: `_eir_global_<mangled_fqn>`. Used by the IR backend for `LoadGlobal`
/// and `StoreGlobal` storage that is distinct from user function labels.
pub fn ir_global_symbol(name: &str) -> String {
    format!("_eir_global_{}", mangle_fqn(name.trim_start_matches('\\')))
}

/// Returns the runtime guard symbol for a `define()` constant name.
///
/// The encoding is stable so duplicate `define()` checks use deterministic BSS
/// sentinel names across compiler versions.
pub fn define_seen_symbol(name: &str) -> String {
    let mut symbol = String::from("_define_seen");
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => symbol.push(byte as char),
            b'_' => symbol.push_str("_u"),
            b'\\' => symbol.push_str("_ns"),
            _ => symbol.push_str(&format!("_x{:02x}", byte)),
        }
    }
    symbol
}

/// Returns the function epilogue symbol for a given PHP function name.
///
/// Format: `_fn_<mangled_fqn>_epilogue`. Appends `_epilogue` to `function_symbol`.
pub fn function_epilogue_symbol(name: &str) -> String {
    format!("{}_epilogue", function_symbol(name))
}

/// Returns the instance method symbol for a class/method pair.
///
/// Format: `_method_<class>_<method>` for alphanumeric names, `_method___<class>___<method>`
/// once either name needs a `mangle_fqn()` escape. Used for virtual dispatch and method table
/// entries; the epilogue label appends `_epilogue` to this symbol.
pub fn method_symbol(class_name: &str, method_name: &str) -> String {
    join_php_symbol("_method", &[class_name, method_name])
}

/// Returns the uniform boxed-Mixed runtime adapter symbol for one class's `__debugInfo()`.
pub fn debug_info_adapter_symbol(class_id: u64) -> String {
    format!("_class_debug_info_adapter_{class_id}")
}

/// Returns the interface method wrapper symbol for a class/interface/method triplet.
///
/// Format: `_ifacewrap_<class_id>_<interface_id>_<method>`. Used by the runtime to route
/// interface method calls through concrete implementation wrappers. The two ids are decimal
/// numbers and join as plain fragments.
pub fn interface_method_wrapper_symbol(
    class_id: u64,
    interface_id: u64,
    method_name: &str,
) -> String {
    let class_id = class_id.to_string();
    let interface_id = interface_id.to_string();
    join_symbol_fragments(
        "_ifacewrap",
        &[&class_id, &interface_id, &mangle_fqn(method_name)],
    )
}

/// Returns the static method symbol for a class/method pair.
///
/// Format: `_static_<class>_<method>`, escaping to `_static___<class>___<method>` when either
/// name is not purely alphanumeric. Used for static method dispatch and method table entries.
pub fn static_method_symbol(class_name: &str, method_name: &str) -> String {
    join_php_symbol("_static", &[class_name, method_name])
}

/// Returns the static property symbol for a class/property pair.
///
/// Format: `_static_prop_<class>_<property>`, escaping to `_static_prop___<class>___<property>`
/// when either name is not purely alphanumeric. Used for static property access and the
/// property lookup table.
pub fn static_property_symbol(class_name: &str, property_name: &str) -> String {
    join_php_symbol("_static_prop", &[class_name, property_name])
}

/// Returns the storage symbol for one function-scoped `static $var` declaration.
///
/// Format: `_static_local_<function>_<variable>`, escaping to
/// `_static_local___<function>___<variable>` when either name is not purely alphanumeric.
/// The dedicated `_static_local` prefix keeps this namespace disjoint from
/// `static_method_symbol()`, which used to share the `_static_` prefix and made a class's
/// static method collide with a same-named function's static local.
pub fn static_local_symbol(function_name: &str, variable_name: &str) -> String {
    join_php_symbol("_static_local", &[function_name, variable_name])
}

/// Returns the one-shot initialization flag symbol paired with `static_local_symbol()`.
///
/// Format: `_static_local_init_<function>_<variable>`. Derived from the same injective
/// (function, variable) encoding rather than by suffixing the storage symbol, so no PHP-legal
/// variable name can spell another static's flag symbol.
pub fn static_local_init_symbol(function_name: &str, variable_name: &str) -> String {
    join_php_symbol("_static_local_init", &[function_name, variable_name])
}

/// Returns the synthetic accessor-method name for a property's `get` hook.
///
/// Format: `__propget_<property>`. The parser compiles a `get { ... }` / `get => expr` hook into a
/// method of this name; codegen routes external `$obj-><property>` reads to a call of it.
pub fn property_hook_get_method(property_name: &str) -> String {
    format!("__propget_{}", property_name)
}

/// Returns the synthetic accessor-method name for a property's `set` hook.
///
/// Format: `__propset_<property>`. The parser compiles a `set { ... }` hook into a method of this
/// name taking the assigned value; codegen routes external `$obj-><property> = v` writes to it.
pub fn property_hook_set_method(property_name: &str) -> String {
    format!("__propset_{}", property_name)
}

/// Returns the enum case symbol for an enum/case pair.
///
/// Format: `_enum_case_<enum>_<case>`, escaping to `_enum_case___<enum>___<case>` when either
/// name is not purely alphanumeric (namespaced enums always take the escaped form). Used for
/// enum case lookup and the enum case table.
pub fn enum_case_symbol(enum_name: &str, case_name: &str) -> String {
    join_php_symbol("_enum_case", &[enum_name, case_name])
}
