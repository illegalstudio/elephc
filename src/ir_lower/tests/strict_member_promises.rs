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

/// Native registration carries structural declarations on all five targets independently from reflection names.
#[test]
fn strict_native_registration_structural_terms_emit_on_every_target() {
    let source = "<?php function nativeMetadataValues(array<int> $values):array<int>{return $values;} \
        class NativeMetadataArray{public array<int> $items=[1];public static array<int> $cached=[2];} \
        $code=$argc>0?'echo 1;':'';eval($code);";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts,true);
    for name in ["macos-aarch64","ios-arm64","ios-sim-arm64","linux-aarch64","linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source,Target::parse(name).unwrap(),profile,false,SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module,false,false).unwrap();
        assert!(assembly.contains(elephc_semantics::native_callable_sources::TYPE_SPEC_PREFIX),"{name}");
    }
}

/// An integrated profile checks native structural member metadata, PHP reflection names and mutation atomicity.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_native_registration_array_members_reflection_and_owners() {
    let fragment="$reflection=new ReflectionFunction('nativeMetadataValues'); \
        echo $reflection->getParameters()[0]->getType()->getName(),':',$reflection->getReturnType()->getName(),':'; \
        $object=new NativeMetadataArray();try{$object->items[]='bad';}catch(TypeError $error){echo 'I';} \
        echo count($object->items),':';try{NativeMetadataArray::$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        echo count(NativeMetadataArray::$cached);";
    let escaped=fragment.replace('\\',"\\\\").replace('\'',"\\'");
    let source=format!("<?php function nativeMetadataValues(array<int> $values):array<int>{{return $values;}} \
        class NativeMetadataArray{{public array<int> $items=[1];public static array<int> $cached=[2];}} \
        $code=$argc>0?'{escaped}':'';eval($code);");
    let profile=SemanticProfile::PERMISSIVE.with(StrictFeature::Casts,true);
    let module=super::semantic_sources::lower_capturing_arguments(&source,Target::detect_host(),profile,true,SourceMode::Php);
    let output=super::native_module::run(&module);
    let stderr=String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(),"{:?}: stdout={}; {stderr}",output.status,String::from_utf8_lossy(&output.stdout));
    assert_eq!(String::from_utf8_lossy(&output.stdout),"array:array:I1:S1","{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"),"{stderr}");
}

/// Actual public profiles preserve logical native producer and declared array input contracts in opaque eval.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_public_profiles_native_eval_logical_inputs_and_contents() {
    let fragment="$values=nativePromiseValues();try{echo $values[0]+1;}catch(TypeError $error){echo '?';} \
        try{$items[]='bad';}catch(TypeError $error){echo 'L';} \
        $object=new NativeContentsPromise();try{$object->items[]='bad';}catch(TypeError $error){echo 'I';} \
        try{NativeContentsPromise::$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        echo count($items),count($object->items),count(NativeContentsPromise::$cached),':'; \
        try{$inferred[]='allowed by mixed';}catch(TypeError $error){echo 'A';} \
        echo count($inferred);$inferred='open';echo ':',$inferred;";
    let escaped=fragment.replace('\\',"\\\\").replace('\'',"\\'");
    let source=format!("<?php class NativeContentsPromise{{public array<int> $items=[1];public static array<int> $cached=[2];}} \
        function nativePromiseValues():array<int>{{return [3];}} array<int> $items=[1];$inferred=[1]; \
        $code=$argc>0?'{escaped}':'';eval($code);");
    let mut failures=Vec::new();
    for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes] {
        let profile=SemanticProfile::PERMISSIVE.with(feature,true);
        assert!(crate::semantics::pending_guarantees(profile).is_empty());
        let module=super::semantic_sources::lower_capturing_arguments(&source,Target::detect_host(),profile,true,SourceMode::Php);
        let output=super::native_module::run(&module);
        let stderr=String::from_utf8_lossy(&output.stderr);
        let expected=if feature==StrictFeature::Mixed {"4LIS111:2:open"} else {"4LIS111:A1:open"};
        let stdout=String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || stdout!=expected || !stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("{feature:?}: {:?}: expected={expected}; stdout={stdout}; {stderr}",output.status));
        }
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

/// Native descriptors retain callable receiver and self/static result domains across opaque eval.
#[cfg(any(all(target_os="macos",target_arch="aarch64"),all(target_os="linux",any(target_arch="x86_64",target_arch="aarch64"))))]
#[test]
fn strict_public_mixed_native_descriptor_bound_result_scopes() {
    let fragment="$made=$factory();$late=$staticFactory();echo get_class($made),':',get_class($late);";
    let escaped=fragment.replace('\\',"\\\\").replace('\'',"\\'");
    let source=format!("<?php class NativeScopedFactory{{public function make():self{{return new self();}} \
        public static function create():static{{return new static();}}}}class NativeScopeChild extends NativeScopedFactory{{}} \
        $object=new NativeScopedFactory();$factory=$object->make(...);$staticFactory=NativeScopeChild::create(...); \
        $code=$argc>0?'{escaped}':'';eval($code);");
    let mut baseline=None;
    for profile in [SemanticProfile::PERMISSIVE,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)] {
        assert!(crate::semantics::pending_guarantees(profile).is_empty());
        let module=super::semantic_sources::lower_capturing_arguments(&source,Target::detect_host(),profile,true,SourceMode::Php);
        let output=super::native_module::run(&module);
        let stderr=String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(),"{:?}: stdout={}; {stderr}",output.status,String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout),"NativeScopedFactory:NativeScopeChild","{stderr}");
        // This test rejects profile-specific retention independently from baseline descriptor ownership.
        let heap=stderr.lines().find(|line|line.starts_with("HEAP DEBUG: leak summary:"))
            .expect("heap-debug execution reports its ownership summary").to_string();
        if let Some(expected)=&baseline { assert_eq!(&heap,expected,"{stderr}"); }
        else { baseline=Some(heap); }
    }
}

/// Public profiles and their example lower through each supported emitter without staged admission.
#[test]
fn strict_public_profiles_examples_emit_on_every_target() {
    for (source,profile) in [
        (include_str!("../../../examples/strict-mode/main.php"),SemanticProfile::all()),
        (include_str!("../../../examples/strict-mixed/main.php"),SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)),
    ] {
        let program=crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        for name in ["macos-aarch64","ios-arm64","ios-sim-arm64","linux-aarch64","linux-x86_64"] {
            let target=Target::parse(name).unwrap();
            let checked=crate::types::check_with_target_and_options(&program,target,
                crate::types::CheckOptions{semantic_profile:profile,..Default::default()},
                &crate::generics::GenericContext::default()).unwrap();
            let module=crate::ir_lower::lower_program(&program,&checked,target,false).unwrap();
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module,false,false).unwrap();
        }
    }
}

/// Native source snapshots retain their own role and shared ABI on the selected target.
fn verify_native_source_snapshot_roles(target_name: &str) {
    let source = "<?php $items=[1];$code=$argc>0?'echo count($items);':'';eval($code);";
    let target = Target::parse(target_name).unwrap();
    for feature in [StrictFeature::Mixed, StrictFeature::ArrayTypes] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        let module = super::semantic_sources::lower_capturing_arguments(
            source, target, profile, false, SourceMode::Php,
        );
        crate::ir::validate_module(&module).unwrap();
        assert!(module.functions.iter().flat_map(|function| &function.instructions).any(|instruction|
            matches!(&instruction.immediate, Some(crate::ir::Immediate::EvalScopeStore(metadata))
                if metadata.role == crate::ir::EvalBindingRole::Source)));
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__elephc_eval_scope_source_type_v1"), "{feature:?}: {target_name}");
    }
}

supported_target_cases!(strict_native_source_snapshot_roles_emit_by_target, verify_native_source_snapshot_roles);
