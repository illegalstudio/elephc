use crate::types::PhpType;

pub(super) fn merge_union_members(members: Vec<PhpType>) -> PhpType {
    let mut flat = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => flat.extend(inner),
            PhpType::Mixed => return PhpType::Mixed,
            other => flat.push(other),
        }
    }
    let mut deduped = Vec::new();
    for member in flat {
        if !deduped.iter().any(|existing| existing == &member) {
            deduped.push(member);
        }
    }
    if deduped.len() == 1 {
        deduped.pop().expect("union member exists")
    } else {
        PhpType::Union(deduped)
    }
}
