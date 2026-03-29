use crate::types::PhpType;

pub(super) fn hash_value_type_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) | PhpType::Callable => 6,
        PhpType::Pointer(_) | PhpType::Void => 0,
    }
}
