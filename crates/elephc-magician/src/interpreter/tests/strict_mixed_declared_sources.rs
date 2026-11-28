//! Purpose:
//! Verifies declaration-backed constructor, member and structural array source metadata.
//!
//! Called from:
//! - Focused runtime Mixed nominal producer regressions.
//!
//! Key details:
//! - self/static return owners remain independent from the current boxed occupant.
//! - Typed properties and method returns supply declarations, while mixed members stay opaque.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Constructors, typed properties and methods retain declared proofs through chained consumers.
#[test]
fn strict_mixed_declared_sources_preserve_nominal_and_member_results() {
    let source="class DeclaredProof {public int $value=4;public static int $cached=5; \
        public function selected():int{return 2;}public static function chosen():int{return 3;} \
        public function same():self{return $this;}public static function create():static{return new static();}} \
        function nominal_proof(DeclaredProof $value):int{return $value->value;} \
        $value=new DeclaredProof();echo nominal_proof($value),':',$value->selected()+1,':',DeclaredProof::chosen()+1, \
        ':',$value->value+1,':',DeclaredProof::$cached+1,':',nominal_proof($value->same()),':',nominal_proof(DeclaredProof::create());return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"4:3:4:5:6:4:4");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Explicit mixed member declarations remain opaque even when their current value is a scalar.
#[test]
fn strict_mixed_declared_sources_keep_mixed_member_obligations() {
    let source="class OpaqueProof {public mixed $value=1;public static mixed $cached=1; \
        public function selected():mixed{return 1;}public static function chosen():mixed{return 1;}} \
        $value=new OpaqueProof();try{echo $value->value+1;}catch(TypeError $error){echo 'P';} \
        try{echo OpaqueProof::$cached+1;}catch(TypeError $error){echo 'S';} \
        try{echo $value->selected()+1;}catch(TypeError $error){echo 'M';} \
        try{echo OpaqueProof::chosen()+1;}catch(TypeError $error){echo 'C';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"PSMC");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Inherited self and static declarations retain their lexical owner and forwarded called class.
#[test]
fn strict_mixed_declared_sources_resolve_inherited_and_interface_results() {
    let source="interface ProofContract {public function selected():int;} \
        class ProofBase {public function same():self{return $this;} \
        public static function create():static{return new static();} \
        public static function forward():static{return self::create();}} \
        class ProofChild extends ProofBase implements ProofContract {public function selected():int{return 2;}} \
        function base_proof(ProofBase $value):int{return 1;} \
        function child_proof(ProofChild $value):int{return 2;} \
        function interface_proof(ProofContract $value):int{return $value->selected()+1;} \
        $value=new ProofChild();echo base_proof($value->same()),':',child_proof(ProofChild::forward()),':',interface_proof($value);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"1:2:3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Declared generic array results retain their element terms without enabling ArrayTypes.
#[test]
fn strict_mixed_declared_sources_preserve_array_element_declarations() {
    let source="function declared_values():array<int>{return [1];}$values=declared_values();return $values[0]+1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert!(output.is_empty());assert_eq!(returned,FakeValue::Int(2));
        }
    }
}

/// Specialized constructors and methods use structural type arguments instead of opaque class text.
#[test]
fn strict_mixed_declared_sources_preserve_generic_class_declarations() {
    let source="class ProofBox<T> {public T $value;public function __construct(T $value){$this->value=$value;} \
        public function get():T{return $this->value;}} \
        function box_proof(ProofBox<int> $value):int{return $value->get()+1;} \
        $value=new ProofBox<int>(1);return box_proof($value);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert!(output.is_empty());assert_eq!(returned,FakeValue::Int(2));
    }
}
