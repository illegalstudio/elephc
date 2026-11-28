//! Purpose:
//! Verifies logical numeric alternatives against independently selected operator domains.
//!
//! Called from:
//! - Focused runtime Mixed, Arithmetic and Coercions composition checks.
//!
//! Key details:
//! - A current integer occupant cannot prove a union that also allows an unsupported kind.
//! - Permissive bitwise and shift operators retain PHP's numeric string domains.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Selected integer-only modulo domains reject a float alternative before consuming its occupant.
#[test]
fn strict_mixed_numeric_domains_follow_selected_arithmetic_signatures() {
    let source="function modulo_proof(int|float $value){try{echo $value%2;}catch(TypeError $error){echo 'A';}return 1;}return modulo_proof(2);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Arithmetic],dispatch);
        assert_eq!(output,"A");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Bitwise-not rejects unsupported boolean alternatives, while numeric string binary domains stay valid.
#[test]
fn strict_mixed_numeric_domains_keep_php_unary_and_binary_domains() {
    let source="function unary_proof(int|bool $value){try{echo ~$value;}catch(TypeError $error){echo 'U';}return 1;} \
        function binary_proof(int|string $value){echo $value&1,':',$value<<1;return 1;} \
        unary_proof(1);echo ':';return binary_proof(1);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"U:1:2");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Logical alternatives use selected scalar and concatenation signatures without enabling other bits.
#[test]
fn strict_mixed_numeric_domains_compose_with_coercions_and_strings() {
    let source="function scalar_domain(int|string $value){try{echo $value+1;}catch(TypeError $error){echo 'C';}} \
        function text_domain(int|string $value){try{echo $value.'!';}catch(TypeError $error){echo 'S';}} \
        scalar_domain(1);text_domain('1');return 1;";
    for dispatch in [false,true] {
        for (features,expected) in [(vec![StrictFeature::Mixed],"21!"),
            (vec![StrictFeature::Coercions,StrictFeature::Strings],"21!"),
            (vec![StrictFeature::Mixed,StrictFeature::Coercions,StrictFeature::Strings],"CS")] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,expected);assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
