//! Purpose:
//! Computes logical operator result alternatives independently of evaluated runtime tags.
//!
//! Called from:
//! - Mixed-aware unary and binary expression evaluation.
//!
//! Key details:
//! - Finite alternatives distribute through reviewed scalar and native class signatures.
//! - Checked integer and stable float results retain their precise kind.

use super::*;
use elephc_generics::Type;
use elephc_semantics::StrictFeature;

/// Flattens represented alternatives while preserving nominal and structural leaf terms.
fn alternatives(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Union(members) => members.iter().flat_map(alternatives).collect(),
        Type::Nullable(inner) => alternatives(inner).into_iter().chain([Type::atom("null")]).collect(),
        ty => vec![ty.clone()],
    }
}

/// Builds a finite result domain without duplicate alternatives or nested union wrappers.
fn union(types: impl IntoIterator<Item=Type>) -> Type {
    let mut result=Vec::new();
    for ty in types { for ty in alternatives(&ty) { if !result.contains(&ty) {result.push(ty);} } }
    match result.len() {0=>Type::atom("never"),1=>result.remove(0),_=>Type::Union(result)}
}

/// Supplies the ordinary PHP numeric domain when values can change result kind.
fn numeric() -> Type { union([Type::atom("int"),Type::atom("float")]) }

/// Identifies source kinds normalized to integers before a successful scalar operation.
fn integer(ty: &Type) -> bool {
    matches!(ty,Type::Atom(name) if matches!(name.as_str(),"int"|"bool"|"true"|"false"|"null"|"void"))
}

/// Computes each logical pair's result before joining the finite represented alternatives.
pub(super) fn binary(op: EvalBinOp,left: &Type,right: &Type,context: &ElephcEvalContext) -> Type {
    let mut result=Vec::new();
    for left in alternatives(left) { for right in alternatives(right) {
        result.push(binary_pair(op,&left,&right,context));
    }}
    union(result)
}

/// Uses the reached operator signature, retaining native handler and scalar precedence.
fn binary_pair(op: EvalBinOp,left: &Type,right: &Type,context: &ElephcEvalContext) -> Type {
    if let Some(result)=mixed_source_proofs::object_result_source(op,left,right,context) {return result;}
    match op {
        EvalBinOp::Concat=>Type::atom("string"),
        EvalBinOp::LogicalAnd|EvalBinOp::LogicalOr|EvalBinOp::LogicalXor|EvalBinOp::LooseEq
            |EvalBinOp::LooseNotEq|EvalBinOp::StrictEq|EvalBinOp::StrictNotEq|EvalBinOp::Lt
            |EvalBinOp::LtEq|EvalBinOp::Gt|EvalBinOp::GtEq=>Type::atom("bool"),
        EvalBinOp::Spaceship|EvalBinOp::Mod|EvalBinOp::ShiftLeft|EvalBinOp::ShiftRight=>Type::atom("int"),
        EvalBinOp::BitAnd|EvalBinOp::BitOr|EvalBinOp::BitXor=>{
            if *left==Type::atom("string") && *right==Type::atom("string") {Type::atom("string")}
            else {Type::atom("int")}
        },
        EvalBinOp::Add if matches!(left,Type::Atom(name)|Type::Apply(name,_) if name=="array")
            && matches!(right,Type::Atom(name)|Type::Apply(name,_) if name=="array")=>{
                if left==right {left.clone()} else {Type::atom("array")}
            },
        EvalBinOp::Div if context.semantic_profile().contains(StrictFeature::Arithmetic)=>Type::atom("float"),
        _ if *left==Type::atom("float") || *right==Type::atom("float")=>Type::atom("float"),
        EvalBinOp::Add|EvalBinOp::Sub|EvalBinOp::Mul if integer(left) && integer(right)
            && context.semantic_profile().contains(StrictFeature::Overflow)=>Type::atom("int"),
        EvalBinOp::Pow if integer(left) && integer(right)
            && context.semantic_profile().contains(StrictFeature::Overflow)
            && context.semantic_profile().contains(StrictFeature::Arithmetic)=>Type::atom("int"),
        _=>numeric(),
    }
}

/// Preserves unary alternatives, including string bitwise results and nominal native handlers.
pub(super) fn unary(op: EvalUnaryOp,operand: &Type,context: &ElephcEvalContext) -> Type {
    union(alternatives(operand).into_iter().map(|operand| {
        if op==EvalUnaryOp::LogicalNot {return Type::atom("bool");}
        if let Some(class)=mixed_source_proofs::operator_class(&operand,context) {
            if op==EvalUnaryOp::BitNot {return Type::atom(class.name);}
            if let Some(result)=mixed_source_proofs::object_result_source(
                if op==EvalUnaryOp::Plus {EvalBinOp::Add} else {EvalBinOp::Sub},
                &Type::atom("int"),&operand,context) {return result;}
        }
        if op==EvalUnaryOp::BitNot {
            return if operand==Type::atom("string") {Type::atom("string")} else {Type::atom("int")};
        }
        if operand==Type::atom("float") {return operand;}
        if integer(&operand) && (op==EvalUnaryOp::Plus || operand!=Type::atom("int")
            || context.semantic_profile().contains(StrictFeature::Overflow)) {return Type::atom("int");}
        numeric()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nominal native operators and mixed string/integer bitwise results retain all logical kinds.
    #[test]
    fn strict_mixed_result_sources_preserve_native_and_bitwise_domains() {
        let context=ElephcEvalContext::new();
        assert_eq!(binary(EvalBinOp::Add,&Type::atom("GMP"),&Type::atom("int"),&context),Type::atom("GMP"));
        assert_eq!(unary(EvalUnaryOp::BitNot,&Type::atom("GMP"),&context),Type::atom("GMP"));
        let source=union([Type::atom("int"),Type::atom("string")]);
        assert_eq!(unary(EvalUnaryOp::BitNot,&source,&context),source);
        assert_eq!(binary(EvalBinOp::BitAnd,&source,&source,&context),source);
    }

    /// Unchecked integers retain PHP promotion while exact floats and checked division remain stable.
    #[test]
    fn strict_mixed_result_sources_preserve_php_promotion_domains() {
        let context=ElephcEvalContext::new();
        assert_eq!(binary(EvalBinOp::Add,&Type::atom("int"),&Type::atom("int"),&context),numeric());
        assert_eq!(unary(EvalUnaryOp::Plus,&Type::atom("int"),&context),Type::atom("int"));
        assert_eq!(unary(EvalUnaryOp::Negate,&Type::atom("int"),&context),numeric());
        assert_eq!(binary(EvalBinOp::Mul,&numeric(),&Type::atom("float"),&context),Type::atom("float"));
    }
}
