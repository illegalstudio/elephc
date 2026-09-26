//! Purpose:
//! Defines parsed and synthetic type expressions before semantic type checking.
//! Represents named, nullable, union, callable, iterable, buffer, and internal array element syntax.
//!
//! Called from:
//! - `crate::parser::stmt::params`, OOP parsers, and downstream type-resolution passes.
//!
//! Key details:
//! - Names remain syntactic until the name resolver canonicalizes namespace and import context.

use crate::names::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
/// One declared type parameter (`T`, `T : Entity`, `K = string`).
///
/// The name is a plain `String` rather than a `Name`: a type parameter resolves against its own
/// declaration's list, never against the namespace, which is why the name resolver carries it
/// through untouched.
pub struct TypeParam {
    pub name: String,
    /// The upper bound (`T : Entity`), checked against the type argument at instantiation.
    ///
    /// Monomorphization does not NEED a bound to compile — each instantiation is a concrete
    /// function either way. The bound is what lets a template state the contract its body
    /// relies on, so a call site that violates it is rejected at the call rather than inside
    /// the instantiated body.
    pub bound: Option<TypeExpr>,
    /// The type argument used when nothing constrains this parameter (`K = string`).
    ///
    /// Without a default, an unconstrained parameter is an error: guessing `mixed` would give
    /// up exactly the storage the annotation exists to pin.
    pub default: Option<TypeExpr>,
    /// How this parameter's instantiations relate to one another (`+T`, `-T`).
    ///
    /// `Invariant` is the default and the only sound choice for a parameter the template can
    /// WRITE: two instantiations are then simply different types, which is also what
    /// monomorphization makes them.
    pub variance: Variance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// The subtyping a type parameter permits between two instantiations of its template.
///
/// Under erasure a variance marker is a statement about one runtime object seen at two types.
/// Under monomorphization `Box<Dog>` and `Box<Animal>` are two real classes, so it is a
/// statement about two of them — and it holds only where they share storage, which is why
/// `Box<Dog>` may widen to `Box<Animal>` (both fields are pointers) while `Box<int>` may not
/// widen to `Box<mixed>` (a register against a boxed tagged cell). See `.plans/generics.md`.
pub enum Variance {
    /// The default: `Box<Dog>` and `Box<Animal>` are unrelated.
    #[default]
    Invariant,
    /// `+T`: `Box<Dog>` may be used where `Box<Animal>` is expected. `T` may not appear in an
    /// input position.
    Covariant,
    /// `-T`: `Sink<Animal>` may be used where `Sink<Dog>` is expected. `T` may not appear in an
    /// output position.
    Contravariant,
}

impl Variance {
    /// The marker as it is written, for a diagnostic that quotes the declaration back.
    pub fn marker(self) -> &'static str {
        match self {
            Variance::Invariant => "",
            Variance::Covariant => "+",
            Variance::Contravariant => "-",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The generic half of a class or interface declaration: what it declares and what it passes on.
///
/// Carried as `Option<Box<GenericDecl>>` so an ordinary declaration — every class that does not
/// mention `<` — costs one null pointer and every pass that predates generics keeps compiling
/// unchanged.
///
/// Like a generic function, a generic class is a TEMPLATE and is never checked or lowered as
/// written: `crate::generics::classes` emits one ordinary `ClassDecl` per distinct type argument
/// list and strips the template before the checker runs.
pub struct GenericDecl {
    /// The type parameters this declaration introduces (`class Box<T : Entity>`).
    pub type_params: Vec<TypeParam>,
    /// Type arguments written on the parent class (`class Small extends Box<int>`), empty when
    /// the parent was written bare. Always empty for an interface, which has no single parent.
    pub extends_args: Vec<TypeExpr>,
    /// Type arguments written on each inherited interface, aligned index by index with the
    /// declaration's `implements` list (for an interface, with its `extends` list). An entry is
    /// empty when that name was written bare, which is why this is a `Vec<Vec<_>>` rather than
    /// a map: alignment is what lets `implements Repository<User>, Countable` keep the arguments
    /// on the interface they were written on.
    pub interface_args: Vec<Vec<TypeExpr>>,
}

impl GenericDecl {
    /// Builds the optional generic half of a declaration, or `None` when nothing is generic.
    ///
    /// The `None` is not an optimization detail, it is the contract: every pass that predates
    /// generics may assume a declaration with `generics: None` behaves exactly as it always
    /// did, and `crate::generics::classes` skips such a declaration without inspecting it.
    /// Both the parser and the AST walker build the field through here so that a declaration
    /// whose last type argument was just substituted away stops being generic in one place.
    pub fn new(
        type_params: Vec<TypeParam>,
        extends_args: Vec<TypeExpr>,
        interface_args: Vec<Vec<TypeExpr>>,
    ) -> Option<Box<GenericDecl>> {
        if type_params.is_empty()
            && extends_args.is_empty()
            && interface_args.iter().all(Vec::is_empty)
        {
            return None;
        }
        Some(Box::new(GenericDecl {
            type_params,
            extends_args,
            interface_args,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Type expression in PHP syntax.
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    /// PHP's literal `false` type, kept distinct from `bool` for flow narrowing.
    False,
    Str,
    Void,
    Never,
    Iterable,
    Array(Box<TypeExpr>),
    /// `array<K, V>`: an associative array with a declared key and value type. Kept distinct
    /// from `Array` because the two have different runtime storage — `Array` is a packed
    /// element vector, `AssocArray` a hash table — and from `Named("array")`, which is the
    /// bare keyword and carries no element contract at all.
    AssocArray {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
    },
    Ptr(Option<Name>),
    Buffer(Box<TypeExpr>),
    Named(Name),
    /// `Box<int>`, `Repository<User>`: a generic class or interface named with type arguments.
    ///
    /// Lives only between parsing and monomorphization. `crate::generics::classes` instantiates
    /// the template under the mangled name and rewrites every mention of this variant to
    /// `Named("Box<int>")` — an ordinary class type whose name happens to carry angle brackets,
    /// exactly as a generic function instantiation becomes an ordinary function named
    /// `identity<int>`. Nothing after that pass can observe it, which is why the checker, the
    /// optimizer and the backend have no notion of a generic class.
    ///
    /// The arguments are a `Vec<TypeExpr>` rather than a rendered string because they can
    /// mention the ENCLOSING declaration's own type parameters: `class Wrapper<T> { private
    /// Box<T> $inner; }` must survive `substitute_type_params`, and substituting inside a name
    /// spelled `"Box<T>"` would be string surgery on nested angle brackets.
    GenericClass {
        name: Name,
        args: Vec<TypeExpr>,
    },
    /// `callable(int): string` — a callable with a DECLARED signature.
    ///
    /// Storage is a plain callable descriptor, identical to a bare `callable`; the signature is
    /// there for the checker. It is what lets a type parameter reach through a callback:
    /// `map<U>(callable(T): U $f)` can bind `U` from the closure the call passes, where a bare
    /// `callable` carries no types and can bind nothing.
    CallableSig {
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
    },
    Nullable(Box<TypeExpr>),
    Union(Vec<TypeExpr>),
    /// PHP 8.1 intersection type `A&B`: a value satisfying every member (all are class/interface
    /// types). Represented for the value as its first member; argument boundaries validate that
    /// every member is satisfied.
    Intersection(Vec<TypeExpr>),
}

impl TypeExpr {
    /// Returns whether this type expression contains PHP's late-bound `static` class type.
    pub fn contains_late_static(&self) -> bool {
        match self {
            TypeExpr::Named(name) => name.as_str().eq_ignore_ascii_case("static"),
            TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
                inner.contains_late_static()
            }
            TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
                members.iter().any(TypeExpr::contains_late_static)
            }
            TypeExpr::GenericClass { args, .. } => {
                args.iter().any(TypeExpr::contains_late_static)
            }
            _ => false,
        }
    }

    /// Collects every class name this type expression names, head and arguments alike.
    ///
    /// Exists for the passes that keep a class alive by name — reachability, prelude pruning —
    /// and have to treat `Box<User>` as naming BOTH `Box` and `User`. Scalars contribute
    /// nothing, so an ordinary annotation returns an empty vec.
    pub fn named_classes(&self) -> Vec<&Name> {
        let mut names = Vec::new();
        self.collect_named_classes(&mut names);
        names
    }

    /// Appends this type expression's class names to `names`, recursing through every member.
    fn collect_named_classes<'a>(&'a self, names: &mut Vec<&'a Name>) {
        match self {
            TypeExpr::Named(name) => names.push(name),
            TypeExpr::Ptr(Some(name)) => names.push(name),
            TypeExpr::GenericClass { name, args } => {
                names.push(name);
                for arg in args {
                    arg.collect_named_classes(names);
                }
            }
            // A signature NAMES classes: `callable(User): Order` mentions two. Missing this arm
            // would let a class named only there be dead-stripped or never autoloaded — the
            // failure the `array<K, V>` sweep already sprang.
            TypeExpr::CallableSig { params, ret } => {
                for param in params {
                    param.collect_named_classes(names);
                }
                ret.collect_named_classes(names);
            }
            TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
                inner.collect_named_classes(names)
            }
            TypeExpr::AssocArray { key, value } => {
                key.collect_named_classes(names);
                value.collect_named_classes(names);
            }
            TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
                for member in members {
                    member.collect_named_classes(names);
                }
            }
            TypeExpr::Int
            | TypeExpr::Float
            | TypeExpr::Bool
            | TypeExpr::False
            | TypeExpr::Str
            | TypeExpr::Void
            | TypeExpr::Never
            | TypeExpr::Iterable
            | TypeExpr::Ptr(None) => {}
        }
    }

    /// Returns whether this type expression mentions any of `type_params`.
    ///
    /// A type parameter is always a bare `Named` with no namespace: the name resolver carries
    /// `type_params` through untouched precisely so the check stays a plain string comparison.
    pub fn mentions_type_param(&self, type_params: &[String]) -> bool {
        match self {
            TypeExpr::Named(name) => type_params.iter().any(|param| param == name.as_str()),
            TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
                inner.mentions_type_param(type_params)
            }
            TypeExpr::AssocArray { key, value } => {
                key.mentions_type_param(type_params) || value.mentions_type_param(type_params)
            }
            TypeExpr::Union(members) | TypeExpr::Intersection(members) => members
                .iter()
                .any(|member| member.mentions_type_param(type_params)),
            // `Box<T>` inside `class Wrapper<T>` mentions `T` in its ARGUMENTS, never in its
            // own name: the head of a generic class type is a class, and a class is not a type
            // parameter. Omitting this arm made a wrapper's own parameter look unmentioned, so
            // the template was treated as already concrete and never instantiated.
            TypeExpr::GenericClass { args, .. } => args
                .iter()
                .any(|arg| arg.mentions_type_param(type_params)),
            // The whole point of the variant: `callable(T): U` is how a type parameter reaches
            // through a callback, so both halves have to be looked at.
            TypeExpr::CallableSig { params, ret } => {
                params
                    .iter()
                    .any(|param| param.mentions_type_param(type_params))
                    || ret.mentions_type_param(type_params)
            }
            _ => false,
        }
    }

    /// Substitutes each type parameter for the concrete type bound to it.
    ///
    /// This is the same operation as [`TypeExpr::substitute_relative_class_types`] with a
    /// different environment: one walks the tree replacing `self`/`static`/`parent` with the
    /// enclosing class, the other replaces `T` with the type argument a call site supplied.
    /// A name absent from `bindings` is returned unchanged, so an ordinary class type inside a
    /// generic signature survives substitution untouched.
    ///
    /// Unlike the relative-class version the match is CASE-SENSITIVE: `self` is a PHP keyword
    /// with one spelling, while `T` and `t` are two distinct type parameters a programmer may
    /// legitimately declare side by side.
    ///
    pub fn substitute_type_params(&self, bindings: &[(String, TypeExpr)]) -> TypeExpr {
        match self {
            TypeExpr::Named(name) => bindings
                .iter()
                .find(|(param, _)| param == name.as_str())
                .map(|(_, bound)| bound.clone())
                .unwrap_or_else(|| self.clone()),
            TypeExpr::Nullable(inner) => {
                TypeExpr::Nullable(Box::new(inner.substitute_type_params(bindings)))
            }
            TypeExpr::Array(inner) => {
                TypeExpr::Array(Box::new(inner.substitute_type_params(bindings)))
            }
            TypeExpr::Buffer(inner) => {
                TypeExpr::Buffer(Box::new(inner.substitute_type_params(bindings)))
            }
            TypeExpr::AssocArray { key, value } => TypeExpr::AssocArray {
                key: Box::new(key.substitute_type_params(bindings)),
                value: Box::new(value.substitute_type_params(bindings)),
            },
            TypeExpr::Union(members) => TypeExpr::Union(
                members
                    .iter()
                    .map(|member| member.substitute_type_params(bindings))
                    .collect(),
            ),
            TypeExpr::Intersection(members) => TypeExpr::Intersection(
                members
                    .iter()
                    .map(|member| member.substitute_type_params(bindings))
                    .collect(),
            ),
            TypeExpr::GenericClass { name, args } => TypeExpr::GenericClass {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| arg.substitute_type_params(bindings))
                    .collect(),
            },
            TypeExpr::CallableSig { params, ret } => TypeExpr::CallableSig {
                params: params
                    .iter()
                    .map(|param| param.substitute_type_params(bindings))
                    .collect(),
                ret: Box::new(ret.substitute_type_params(bindings)),
            },
            _ => self.clone(),
        }
    }

    /// Rewrites the relative class types `self`/`static` to `self_class` and `parent` to
    /// `parent_class`, recursing through nullable, union, array, and buffer members.
    ///
    /// `self` and `static` both resolve to the enclosing class (declaring class for `static`);
    /// `parent` resolves to its parent, or is left untouched when `parent_class` is `None` so a
    /// later pass can report "no parent class". The match on the keyword is case-insensitive,
    /// and any non-relative named type is returned unchanged. Applied after inheritance/trait
    /// flattening, when the concrete enclosing class is finally known.
    pub fn substitute_relative_class_types(
        &self,
        self_class: &str,
        parent_class: Option<&str>,
    ) -> TypeExpr {
        match self {
            TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
                "self" | "static" => TypeExpr::Named(Name::unqualified(self_class)),
                "parent" => match parent_class {
                    Some(parent) => TypeExpr::Named(Name::unqualified(parent)),
                    None => self.clone(),
                },
                _ => self.clone(),
            },
            TypeExpr::Nullable(inner) => TypeExpr::Nullable(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            TypeExpr::Union(members) => TypeExpr::Union(
                members
                    .iter()
                    .map(|member| member.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
            ),
            TypeExpr::Intersection(members) => TypeExpr::Intersection(
                members
                    .iter()
                    .map(|member| member.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
            ),
            TypeExpr::Array(inner) => TypeExpr::Array(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            TypeExpr::Buffer(inner) => TypeExpr::Buffer(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            TypeExpr::GenericClass { name, args } => TypeExpr::GenericClass {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| arg.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
            },
            TypeExpr::CallableSig { params, ret } => TypeExpr::CallableSig {
                params: params
                    .iter()
                    .map(|param| param.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
                ret: Box::new(ret.substitute_relative_class_types(self_class, parent_class)),
            },
            other => other.clone(),
        }
    }

    /// Resolves relative class types in a method return while preserving late-bound `static`.
    ///
    /// `self` and `parent` are lexical declaration types and can be replaced immediately.
    /// `static` must remain symbolic until a call site supplies the receiver type.
    pub fn substitute_method_return_relative_types(
        &self,
        self_class: &str,
        parent_class: Option<&str>,
    ) -> TypeExpr {
        match self {
            TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("static") => self.clone(),
            TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("self") => {
                TypeExpr::Named(Name::unqualified(self_class))
            }
            TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("parent") => {
                parent_class
                    .map(|parent| TypeExpr::Named(Name::unqualified(parent)))
                    .unwrap_or_else(|| self.clone())
            }
            TypeExpr::Nullable(inner) => TypeExpr::Nullable(Box::new(
                inner.substitute_method_return_relative_types(self_class, parent_class),
            )),
            TypeExpr::Union(members) => TypeExpr::Union(
                members
                    .iter()
                    .map(|member| {
                        member.substitute_method_return_relative_types(self_class, parent_class)
                    })
                    .collect(),
            ),
            TypeExpr::Intersection(members) => TypeExpr::Intersection(
                members
                    .iter()
                    .map(|member| {
                        member.substitute_method_return_relative_types(self_class, parent_class)
                    })
                    .collect(),
            ),
            TypeExpr::Array(inner) => TypeExpr::Array(Box::new(
                inner.substitute_method_return_relative_types(self_class, parent_class),
            )),
            TypeExpr::Buffer(inner) => TypeExpr::Buffer(Box::new(
                inner.substitute_method_return_relative_types(self_class, parent_class),
            )),
            other => other.clone(),
        }
    }
}
