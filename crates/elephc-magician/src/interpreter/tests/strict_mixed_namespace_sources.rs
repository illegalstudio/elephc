//! Purpose:
//! Verifies source declarations and predicate views across PHP namespace fallback rules.
//!
//! Called from:
//! - Focused Mixed namespaced call and selected boolean source regressions.
//!
//! Key details:
//! - User declarations shadow global fallback without inheriting builtin predicate semantics.
//! - Imported and fully qualified builtin identities retain their reviewed source contracts.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Namespaced calls preserve their declarations while the global predicate fallback narrows the source.
#[test]
fn strict_mixed_namespace_sources_keep_fallback_producers_and_predicates() {
    let source="namespace SourceFallback;function origin():mixed{return 1;}function known():int{return 3;} \
        $value=origin();try{echo $value+1;}catch(\\TypeError $error){echo 'M';} \
        if(is_int($value)){echo $value+1;}echo known()+1;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Bools]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"M24");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// A shadowing user predicate cannot manufacture a builtin type proof.
#[test]
fn strict_mixed_namespace_sources_respect_user_predicate_shadowing() {
    let source="namespace SourceShadow;function origin():mixed{return 1;} \
        function is_int($value):bool{return true;}$value=origin(); \
        if(is_int($value)){try{echo $value+1;}catch(\\TypeError $error){echo 'S';}} \
        if(\\is_int($value)){echo $value+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Bools],dispatch);
        assert_eq!(output,"S2");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Imported and qualified case-insensitive aliases narrow the same logical source.
#[test]
fn strict_mixed_namespace_sources_preserve_imported_predicate_identities() {
    let source="namespace SourceImports;use function \\is_int as number_guard; \
        function origin():mixed{return 1;}$value=origin(); \
        if(number_guard($value)){echo $value+1;}$value=origin(); \
        if(\\IS_INTEGER($value)){echo $value+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Bools],dispatch);
        assert_eq!(output,"22");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Namespaced explicit mixed returns keep open contents while undeclared returns remain inferred.
#[test]
fn strict_mixed_namespace_sources_preserve_independent_array_origin_roles() {
    let source="namespace SourceArrayRoles;function open():mixed{return 1;}function unknown(){return 1;} \
        $open=[open()];$open[]='allowed';$closed=[unknown()]; \
        try{$closed[]='bad';}catch(\\TypeError $error){echo 'C';}return count($open)+count($closed);";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::ArrayTypes],vec![StrictFeature::ArrayTypes,StrictFeature::Mixed],
            vec![StrictFeature::ArrayTypes,StrictFeature::Locals]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"C");assert_eq!(returned,FakeValue::Int(3));
        }
    }
}
