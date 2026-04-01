use crate::types::PhpType;

pub(super) fn hash_value_type_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => 0,
    }
}
