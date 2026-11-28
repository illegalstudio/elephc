//! Purpose:
//! Normalizes proven dynamic static-property statements into the existing static dispatcher.
//!
//! Called from:
//! - Mixed property statement execution before any mutation occurs.
//!
//! Key details:
//! - Selectors evaluate once and RHS/reference/index expressions remain unevaluated until normal dispatch.

use super::*;

/// Recognizes dynamic class mutation statements without inspecting or evaluating their operands.
pub(in crate::interpreter) fn handles(stmt: &EvalStmt) -> bool {
    matches!(stmt,
        EvalStmt::DynamicStaticPropertySet {..}
        | EvalStmt::DynamicStaticPropertyReferenceBind {..}
        | EvalStmt::DynamicStaticPropertyArrayAppend {..}
        | EvalStmt::DynamicStaticPropertyArraySet {..}
        | EvalStmt::DynamicStaticPropertyIncDec {..}
        | EvalStmt::DynamicStaticPropertyNameSet {..}
        | EvalStmt::DynamicStaticPropertyNameReferenceBind {..}
        | EvalStmt::DynamicStaticPropertyNameArrayAppend {..}
        | EvalStmt::DynamicStaticPropertyNameArraySet {..}
        | EvalStmt::DynamicStaticPropertyNameIncDec {..}
        | EvalStmt::UnsetDynamicStaticProperty {..}
        | EvalStmt::UnsetDynamicStaticPropertyName {..})
}

/// Selects a class/member once, then delegates mutation and ownership to the ordinary static statement.
pub(in crate::interpreter) fn execute(stmt: &EvalStmt,context: &mut ElephcEvalContext,scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl,EvalStatus> {
    let (class,selector,fixed)=match stmt {
        EvalStmt::DynamicStaticPropertySet {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyReferenceBind {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyArrayAppend {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyArraySet {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyIncDec {class_name,property,..}
        | EvalStmt::UnsetDynamicStaticProperty {class_name,property,..} => (class_name,None,Some(property.as_str())),
        EvalStmt::DynamicStaticPropertyNameSet {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyNameReferenceBind {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyNameArrayAppend {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyNameArraySet {class_name,property,..}
        | EvalStmt::DynamicStaticPropertyNameIncDec {class_name,property,..}
        | EvalStmt::UnsetDynamicStaticPropertyName {class_name,property,..} => (class_name,Some(property),None),
        _=>return Err(EvalStatus::UnsupportedConstruct),
    };
    with_void(class,selector,context,scope,values,|class,selector,context,scope,values| {
        let property=fixed.or(selector).ok_or(EvalStatus::RuntimeFatal)?.to_string();
        let selected=match stmt {
            EvalStmt::DynamicStaticPropertySet {value,..}
        | EvalStmt::DynamicStaticPropertyNameSet {value,..} => EvalStmt::StaticPropertySet {class_name:class.into(),property,value:value.clone()},
            EvalStmt::DynamicStaticPropertyReferenceBind {source,..}
        | EvalStmt::DynamicStaticPropertyNameReferenceBind {source,..} => EvalStmt::StaticPropertyReferenceBind {class_name:class.into(),property,source:source.clone()},
            EvalStmt::DynamicStaticPropertyArrayAppend {value,..}
        | EvalStmt::DynamicStaticPropertyNameArrayAppend {value,..} => EvalStmt::StaticPropertyArrayAppend {class_name:class.into(),property,value:value.clone()},
            EvalStmt::DynamicStaticPropertyArraySet {index,op,value,..}
        | EvalStmt::DynamicStaticPropertyNameArraySet {index,op,value,..} => EvalStmt::StaticPropertyArraySet {class_name:class.into(),property,index:index.clone(),op:*op,value:value.clone()},
            EvalStmt::DynamicStaticPropertyIncDec {increment,..}
        | EvalStmt::DynamicStaticPropertyNameIncDec {increment,..} => EvalStmt::StaticPropertyIncDec {class_name:class.into(),property,increment:*increment},
            EvalStmt::UnsetDynamicStaticProperty {..}
        | EvalStmt::UnsetDynamicStaticPropertyName {..}=>
                EvalStmt::UnsetStaticProperty {class_name:class.into(),property},
            _=>return Err(EvalStatus::UnsupportedConstruct),
        };
        match crate::interpreter::statements::execute_property_stmt(&selected,context,scope,values)? {
            EvalControl::None=>Ok(()),_=>Err(EvalStatus::RuntimeFatal),
        }
    })?;
    Ok(EvalControl::None)
}
