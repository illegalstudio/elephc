use crate::names::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Never,
    Iterable,
    Ptr(Option<Name>),
    Buffer(Box<TypeExpr>),
    Named(Name),
    Nullable(Box<TypeExpr>),
    Union(Vec<TypeExpr>),
}
