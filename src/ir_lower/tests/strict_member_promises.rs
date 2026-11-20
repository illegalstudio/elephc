//! Purpose:
//! Verifies checked property reference layout for Mixed and ArrayTypes on every target.
//!
//! Called from:
//! - Focused native declaration-promise and eval admission regressions.
//!
//! Key details:
//! - Shared profile selection must preserve the existing target-aware reference ABI.

use crate::{codegen_support::platform::Target,source::SourceMode};
use elephc_semantics::{SemanticProfile,StrictFeature};

/// Mixed and ArrayTypes scalar property aliases select valid managed layouts on every target.
#[test]
fn strict_member_promises_reference_profiles_emit_on_every_target() {
    let source="<?php class NativeMemberPromise{public int $number=1;public static int $cached=1;} \
        $object=new NativeMemberPromise();$instance=2;$static=3; \
        $instance=&$object->number;$static=&NativeMemberPromise::$cached; \
        $instance=12;$static=13;echo $object->number,':',NativeMemberPromise::$cached;";
    for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes] {
        for name in ["macos-aarch64","ios-arm64","ios-sim-arm64","linux-aarch64","linux-x86_64"] {
            let profile=SemanticProfile::PERMISSIVE.with(feature,true);
            let module=super::semantic_sources::lower_capturing_arguments(source,Target::parse(name).unwrap(),profile,false,SourceMode::Php);
            assert!(module.required_runtime_features.native_reference_validation);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module,false,false).unwrap();
        }
    }
}

/// Native aliases retain their scalar declaration and release their canonical owners.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_member_promises_native_reference_profiles_and_owners() {
    let source="<?php class NativeMemberValue{public int $number=1;public static int $cached=1;} \
        $object=new NativeMemberValue();$instance=2;$static=3; \
        $instance=&$object->number;$static=&NativeMemberValue::$cached; \
        $instance=12;$static=13;echo $object->number,':',NativeMemberValue::$cached;";
    for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes] {
        let profile=SemanticProfile::PERMISSIVE.with(feature,true);
        let module=super::semantic_sources::lower_capturing_arguments(source,Target::detect_host(),profile,true,SourceMode::Php);
        let output=super::native_module::run(&module);
        let stderr=String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(),"{feature:?}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout),"12:13","{feature:?}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"),"{feature:?}: {stderr}");
    }
}

/// An integrated checked-reference profile verifies native prospective admission and reentrant callbacks.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_member_promises_native_eval_initial_admission_and_owners() {
    let fragment="class NativeAdmissionHolder{public int $number=1;public string $text='old';} \
        class NativeAdmissionString{public mixed $mutator;public function __construct($mutator){$this->mutator=$mutator;} \
            public function __toString():string{$callback=$this->mutator;if(is_callable($callback)){$callback();}echo 'C';return '12';}} \
        $holder=new NativeAdmissionHolder();$number='12';$holder->number=&$number;echo gettype($number),':',$holder->number; \
        $source=null;$mutator=function()use(&$source):void{$source=[];}; \
        $source=new NativeAdmissionString($mutator);$holder->text=&$source;echo ':',gettype($source),':',$holder->text;";
    let escaped=fragment.replace('\\',"\\\\").replace('\'',"\\'");
    let source=format!("<?php $code=$argc>0?'{escaped}':'';eval($code);");
    let profile=SemanticProfile::PERMISSIVE.with(StrictFeature::Casts,true);
    let module=super::semantic_sources::lower_capturing_arguments(&source,Target::detect_host(),profile,true,SourceMode::Php);
    let output=super::native_module::run(&module);
    let stderr=String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(),"{:?}: stdout={}; {stderr}",output.status,String::from_utf8_lossy(&output.stdout));
    assert_eq!(String::from_utf8_lossy(&output.stdout),"integer:12C:string:12","{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"),"{stderr}");
}

/// Live structural property aliases constrain interior writes and default floats keep their promised kind.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_member_promises_native_eval_array_aliases_and_float_defaults() {
    let fragment="class NativeArrayPromise{public array<int> $items=[1];public static array<int> $cached=[2]; \
        public float $number=0;public static float $floating=0;} \
        class NativeInvalidArrayDefault{public array<int> $items=['bad'];} \
        try{$bad=new NativeInvalidArrayDefault();}catch(TypeError $error){echo 'F';} \
        $object=new NativeArrayPromise();array $instance=[1];array $cached=[2]; \
        try{$object->items[]='bad';}catch(TypeError $error){echo 'D';} \
        try{NativeArrayPromise::$cached[]='bad';}catch(TypeError $error){echo 'E';} \
        $leaf=1;$staticLeaf=2;$object->items=[&$leaf];NativeArrayPromise::$cached=[&$staticLeaf]; \
        try{$leaf='bad';}catch(TypeError $error){echo str_contains($error->getMessage(),'--strict-array-types')?'wrong':'R';} \
        try{$staticLeaf='bad';}catch(TypeError $error){echo 'T';} \
        $object->items=[1];NativeArrayPromise::$cached=[2];$leaf='released';$staticLeaf='released'; \
        $object->items=&$instance;NativeArrayPromise::$cached=&$cached; \
        try{$instance[]='bad';}catch(TypeError $error){echo 'I';} \
        try{$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        echo count($instance),count($cached),':',gettype($object->number),':',gettype(NativeArrayPromise::$floating); \
        function visitNativeArrayPromise(&$items):void{$items[]=3;} \
        visitNativeArrayPromise($instance);unset($object);$instance[]='allowed';echo ':',count($instance);";
    let escaped=fragment.replace('\\',"\\\\").replace('\'',"\\'");
    let source=format!("<?php $code=$argc>0?'{escaped}':'';eval($code);");
    let profile=SemanticProfile::PERMISSIVE.with(StrictFeature::Casts,true);
    let module=super::semantic_sources::lower_capturing_arguments(&source,Target::detect_host(),profile,true,SourceMode::Php);
    let output=super::native_module::run(&module);
    let stderr=String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(),"{:?}: stdout={}; {stderr}",output.status,String::from_utf8_lossy(&output.stdout));
    assert_eq!(String::from_utf8_lossy(&output.stdout),"FDERTIS11:double:double:3","{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"),"{stderr}");
}
