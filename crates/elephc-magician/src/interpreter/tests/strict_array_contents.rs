//! Purpose:
//! Exercises array-content constraints without pinning a binding's whole value kind.
//!
//! Called from:
//! - Focused inferred-array source contract and metadata regressions.
//!
//! Key details:
//! - Existing generic validators own candidate checks and keep failed stores unchanged.
//! - Conditional metadata is separate from ordinary typed-local declarations.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Installs a generated conditional schema before executing the fixture's source writes.
fn run_contents(source: &str, schema: &str, features: &[StrictFeature]) -> (String, FakeValue) {
    run_source(source, Some(schema), features, false)
}

/// Runs either explicit metadata or automatic producer inference with complete owner cleanup.
pub(super) fn run_source(source: &str, schema: Option<&str>, features: &[StrictFeature], dispatch: bool) -> (String, FakeValue) {
    let schema = schema.map(|schema| crate::parser::generics::parse_type_string(schema).unwrap());
    run_source_with_type(source, schema, features, dispatch)
}

/// Installs trusted generated type terms independently from PHP's narrower source type grammar.
pub(super) fn run_source_with_type(source: &str, schema: Option<elephc_generics::Type>,
    features: &[StrictFeature], dispatch: bool,
) -> (String, FakeValue) {
    let mut context = ElephcEvalContext::new();
    let profile = features.iter().fold(SemanticProfile::PERMISSIVE,
        |profile, feature| profile.with(*feature, true));
    context.install_semantic_profile(profile).unwrap();
    let mut scope = ElephcEvalScope::new();
    if let Some(schema) = schema {
        let contract = crate::eval_ir::array_contents_type(schema).unwrap();
        scope.infer_local_type("items".to_string(), crate::eval_ir::parameter_type(&contract));
    }
    let mut values = FakeOps::default();
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile).unwrap();
    let executed = if dispatch {
        bytecode::try_execute(&mut context, &program, &mut scope, &mut values).and_then(|result|
            result.map(Ok).unwrap_or_else(|| execute_statements(program.statements(), &mut context, &mut scope, &mut values)))
    } else { execute_statements(program.statements(), &mut context, &mut scope, &mut values) };
    let result = executed.unwrap_or_else(|error| panic!("{source}: {error:?}; {}", values.output));
    let EvalControl::Return(result) = result else { panic!("fixture returns its observed value"); };
    let returned = values.get(result); values.release(result).unwrap();
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    assert!(context.take_pending_throw().is_none());
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: owner{id}"); }
    (values.output, returned)
}

/// Non-array replacements stay legal while array candidates retain exact element contracts.
#[test]
fn strict_array_contents_stores_preserve_independent_value_kinds() {
    let source = "$items=[1];try{$items[]='bad';}catch(TypeError $error){echo 'E';} \
        $items=7;echo ':',gettype($items);$items=null;echo ':',gettype($items); \
        $items=[2];try{$items=['bad'];}catch(TypeError $error){echo ':R';}return $items[0];";
    for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::Nullability]] {
        let (output, returned) = run_contents(source, "array<int>", &features);
        assert_eq!(output, "E:integer:NULL:R"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Semantic integer keys allow hash gaps while the independent exact-key policy retains strings.
#[test]
fn strict_array_contents_keys_preserve_layout_and_normalization_policy() {
    for (features, expected) in [(vec![StrictFeature::ArrayTypes], "K"),
        (vec![StrictFeature::ArrayTypes, StrictFeature::ArrayKeys], "NK")] {
        let source = "$items=[2=>1];try{$items['3']=2;}catch(TypeError $error){echo 'N';} \
            try{$items['other']=3;}catch(TypeError $error){echo 'K';}return $items[2];";
        let (output, returned) = run_contents(source, "array<int>", &features);
        assert_eq!(output, expected); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Explicit mixed/union elements remain open and aliases cannot bypass a closed source schema.
#[test]
fn strict_array_contents_aliases_and_explicit_element_alternatives() {
    for schema in ["array<int|string>", "array<mixed>"] {
        let (output, returned) = run_contents("$items=[1];$items[]='allowed';return $items[1];",
            schema, &[StrictFeature::ArrayTypes]);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::String("allowed".to_string()));
    }
    let source = "$items=[1];$alias=&$items;try{$alias[]='bad';}catch(TypeError $error){echo 'A';} \
        $alias=7;$alias=[2];return $items[0];";
    let (output, returned) = run_contents(source, "array<int>", &[StrictFeature::ArrayTypes]);
    assert_eq!(output, "A"); assert_eq!(returned, FakeValue::Int(2));
}

/// Fresh literal arrays retain contents through copies, alias writes and unrelated kind changes.
#[test]
fn strict_array_origins_automatic_locals_copies_and_aliases() {
    for dispatch in [false, true] {
        let source = "$items=[1];$copy=$items;$alias=&$items; \
            try{$copy[]='bad';}catch(TypeError $error){echo 'C';} \
            try{$alias[]='bad';}catch(TypeError $error){echo 'A';} \
            $items=7;$items=null;$items=[2]; \
            try{$items=['bad'];}catch(TypeError $error){echo 'R';}return $items[0];";
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "CAR"); assert_eq!(returned, FakeValue::Int(2));
        let source = "$items=[1];$alias=&$items;$copy=$alias; \
            try{$copy[]='bad';}catch(TypeError $error){echo 'A';}return $items[0];";
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "A"); assert_eq!(returned, FakeValue::Int(1));
        let source = "$items=[1];$copy=$items; \
            try{$items=7;}catch(TypeError $error){echo 'L';} \
            try{$copy[]='bad';}catch(TypeError $error){echo 'C';}return $copy[0];";
        let (output, returned) = run_source(source, None,
            &[StrictFeature::ArrayTypes, StrictFeature::Locals], dispatch);
        assert_eq!(output, "LC"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Inferred heterogeneous initializers reject before replacement and retire every candidate owner.
#[test]
fn strict_array_origins_heterogeneous_initializers_are_atomic() {
    let source = "$items=7;try{$items=[1,'bad'];}catch(TypeError $error){echo 'H';} \
        $other=8;try{$other=[[1],['bad']];}catch(TypeError $error){echo 'N';} \
        return $items+$other;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "HN"); assert_eq!(returned, FakeValue::Int(15));
    }
}

/// Declared mixed and bare-array returns keep intentional openness after assignment and copying.
#[test]
fn strict_array_origins_declared_open_producers_stay_open() {
    let source = "function open_array():array{return [1];} \
        function open_mixed():mixed{return [1];} \
        $items=open_array();$items[]='allowed';$copy=$items;$copy[]=false; \
        $other=open_mixed();$other[]='allowed';return count($copy)+count($other);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(5));
    }
}

/// The actual bytecode store preserves a copied array contract and rejects before publication.
#[test]
fn strict_array_origins_vm_copy_replacement_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayTypes, true);
    let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let setup = crate::parser::parse_fragment_with_semantics(
        b"$items=[1];$candidate=['bad'];", profile).unwrap();
    execute_statements(setup.statements(), &mut context, &mut scope, &mut values).unwrap();
    let program = crate::parser::parse_fragment_with_semantics(
        b"$copy=$items;$copy=$candidate;", profile).unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    assert!(matches!(bytecode::try_execute(&mut context, &program, &mut scope, &mut values),
        Err(EvalStatus::UncaughtThrowable)));
    assert!(bytecode::last_execution_used_vm());
    let thrown = context.take_pending_throw().unwrap();
    assert!(values.object_is_a(thrown, "TypeError", false).unwrap()); values.release(thrown).unwrap();
    let copy = scope.visible_cell("copy").unwrap();
    let index = values.int(0).unwrap(); let element = values.array_get(copy, index).unwrap();
    assert_eq!(values.get(element), FakeValue::Int(1));
    values.release(index).unwrap(); values.release(element).unwrap();
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "owner{id}"); }
}

/// Empty arrays acquire one content contract only after a successful append or indexed write.
#[test]
fn strict_array_origins_empty_first_writes_and_aliases() {
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::Locals]] {
            let source = "$items=[];$before=$items;$alias=&$items;$alias[]=1; \
                try{$items[]='bad';}catch(TypeError $error){echo 'A';} \
                $before[]='allowed';try{$before[]=2;}catch(TypeError $error){echo 'C';} \
                $after=$alias;try{$after[]=false;}catch(TypeError $error){echo 'B';}return $items[0];";
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "ACB"); assert_eq!(returned, FakeValue::Int(1));
        }
        let source = "$items=[];$items['key']=1;$items['other']=2; \
            try{$items[0]=3;}catch(TypeError $error){echo 'K';} \
            try{$items['bad']='bad';}catch(TypeError $error){echo 'V';}return count($items);";
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "KV"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Failed keys and nested candidates leave inference pending and permit a later valid first write.
#[test]
fn strict_array_origins_empty_failed_candidates_do_not_publish() {
    let source = "$items=[];try{$items[false]=1;}catch(TypeError $error){echo 'K';} \
        try{$items[]=[1,'bad'];}catch(TypeError $error){echo 'H';} \
        $items[]='valid';try{$items[]=2;}catch(TypeError $error){echo 'T';}return $items[0];";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None,
            &[StrictFeature::ArrayTypes, StrictFeature::ArrayKeys], dispatch);
        assert_eq!(output, "KHT"); assert_eq!(returned, FakeValue::String("valid".to_string()));
    }
}

/// Whole-array replacement refines an empty origin while explicit mixed first values remain open.
#[test]
fn strict_array_origins_empty_replacements_and_explicit_mixed_values() {
    let source = "function open_value():mixed{return 1;} \
        $items=[];$items[]=open_value();$items[]='allowed'; \
        $other=[];$other=[1];try{$other[]='bad';}catch(TypeError $error){echo 'R';} \
        return count($items)+count($other);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "R"); assert_eq!(returned, FakeValue::Int(3));
    }
}

/// Parameter declarations own element alternatives without imposing unrelated local kind stability.
#[test]
fn strict_array_origins_parameter_declarations_and_variadics() {
    let source = "function closed(array<int> $items){ \
        try{$items[]='bad';}catch(TypeError $error){echo 'T';} \
        $items=7;$items=null;$items=[2];return $items[0];} \
        function open_parameter(array $items){$items[]=1;$items[]='allowed';return count($items);} \
        function variadic_items(int ...$items){try{$items[]='bad';}catch(TypeError $error){echo 'V';}return count($items);} \
        echo closed([1]),open_parameter([]),variadic_items(1,2);return 7;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "T22V2"); assert_eq!(returned, FakeValue::Int(7));
    }
}

/// Captured schemas preserve copies and permit null stores independently from Nullability.
#[test]
fn strict_array_origins_closure_copies_pending_and_null_stores() {
    let source = "$items=[1];$closed=function()use($items){ \
        try{$items[]='bad';}catch(TypeError $error){echo 'C';} \
        $items=7;$items=null;return 1;};echo $closed(); \
        $empty=[];$pending=function()use($empty){$empty[]=1; \
        try{$empty[]='bad';}catch(TypeError $error){echo 'P';}return count($empty);}; \
        echo $pending(),count($empty);return count($items);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::Nullability]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "C1P10"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// An untyped by-reference callee cannot bypass a caller's established array contents.
#[test]
fn strict_array_origins_reference_parameters_check_before_mutation() {
    let source = "function corrupt(&$value){$value[]='bad';} \
        $items=[1];try{corrupt($items);}catch(TypeError $error){echo 'R';}return count($items);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "R"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Explicit heterogeneous typed initializers retain their declared alternatives through copying.
#[test]
fn strict_array_origins_typed_heterogeneous_initializers_preserve_context() {
    let source = "array<int|string> $items=[1,'allowed'];$copy=$items;$copy[]='more'; \
        array<mixed> $open=[1,'allowed'];$open[]=false;return count($copy)+count($open);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(6));
    }
}

/// Equal runtime scalar kinds retain distinct inferred and explicitly mixed source views.
#[test]
fn strict_array_origins_scalar_sources_are_independent_from_locals() {
    let source = "function mixed_scalar():mixed{return 1;} \
        $number=1;$copy=$number;$number='changed'; \
        $items=[];$items[]=$copy;try{$items[]='bad';}catch(TypeError $error){echo 'I';} \
        $open=mixed_scalar();$open_copy=$open;$other=[];$other[]=$open_copy;$other[]='allowed'; \
        $strings=[];$strings[]=$number;try{$strings[]=2;}catch(TypeError $error){echo 'S';} \
        return count($items)+count($other)+count($strings);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "IS"); assert_eq!(returned, FakeValue::Int(4));
    }
}

/// Generation changes retire cached source views instead of reusing the old scalar proof.
#[test]
fn strict_array_origins_source_views_follow_scope_generations() {
    let context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let first = values.int(1).unwrap(); scope.set("source", first, ScopeCellOwnership::Owned);
    let integer = EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false);
    scope.record_local_source_view("source".to_string(), integer.clone(), 0);
    assert_eq!(scope.local_source_view("source", &context), Some(&integer));
    scope.mark_reference_changed("source"); assert!(scope.local_source_view("source", &context).is_none());
    scope.record_local_source_view("source".to_string(), integer, 0);
    let replacement = values.string("new").unwrap();
    let old = scope.set("source", replacement, ScopeCellOwnership::Owned).unwrap(); values.release(old).unwrap();
    assert!(scope.local_source_view("source", &context).is_none());
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for owners in values.cell_owners.values() { assert_eq!(*owners, 0); }
}

/// Scalar declarations supply array element views even when stable Locals enforcement is disabled.
#[test]
fn strict_array_origins_scalar_parameter_source_views() {
    let source = "function inferred_parameter(int $number){$items=[];$items[]=$number; \
        try{$items[]='bad';}catch(TypeError $error){echo 'I';}return count($items);} \
        function mixed_parameter(mixed $number){$items=[];$items[]=$number;$items[]='allowed';return count($items);} \
        echo inferred_parameter(1),mixed_parameter(1);return 7;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "I12"); assert_eq!(returned, FakeValue::Int(7));
    }
}

/// Captured scalar views inform first writes without becoming an implicit stable local contract.
#[test]
fn strict_array_origins_scalar_capture_views_do_not_enable_locals() {
    let source = "function mixed_capture_value():mixed{return 1;} \
        $number=1;$open=mixed_capture_value();$capture=function()use($number,$open){ \
        $items=[];$items[]=$number;try{$items[]='bad';}catch(TypeError $error){echo 'I';} \
        $number='changed';$other=[];$other[]=$number; \
        try{$other[]=2;}catch(TypeError $error){echo 'S';} \
        $mixed=[];$mixed[]=$open;$mixed[]='allowed';return count($mixed);};echo $capture();return $number;";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::Nullability]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "IS2"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// Literal entries retain declared mixed/union alternatives instead of using incidental cell tags.
#[test]
fn strict_array_origins_literal_entries_preserve_open_producers() {
    let source = "function literal_mixed_value():mixed{return 1;} \
        function literal_union_value(int|string $value){$items=[$value];$items[]='allowed';$items[]=2; \
        try{$items[]=false;}catch(TypeError $error){echo 'U';}return count($items);} \
        $open=literal_mixed_value();$items=[$open];$items[]='allowed'; \
        echo literal_union_value(1);return count($items);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "U3"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Literal effects precede later source snapshots and duplicate keys retain only the final entry.
#[test]
fn strict_array_origins_literal_effects_and_duplicate_keys() {
    let source = "$key=1;$items=[($key='new'),$key];$items[]='more'; \
        $map=['key'=>1,'key'=>'last'];$map['other']='more'; \
        try{$map['bad']=2;}catch(TypeError $error){echo 'D';} \
        $value=1;$keyed=[($value='key')=>$value];$keyed['next']='valid'; \
        $before=7;try{$before=[1,($effect='s'),($later='tail')];}catch(TypeError $error){echo 'H';} \
        echo $effect,$later,$before;return count($items)+count($map)+count($keyed);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "DHstail7"); assert_eq!(returned, FakeValue::Int(7));
    }
}

/// Nested literal sources and existing declared destinations retain explicit element alternatives.
#[test]
fn strict_array_origins_nested_literal_and_destination_contexts() {
    let source = "function nested_mixed_value():mixed{return 1;} \
        $items=[[nested_mixed_value()]];$items[]=['allowed']; \
        $empty=[[],[1]];$empty[]=[2]; \
        array<int|string> $typed=[];$typed=[1,'allowed'];$typed[]=2; \
        return count($items)+count($empty)+count($typed);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(8));
    }
}

/// Explicit nullable alternatives and inherited object values keep their compatible source schema.
#[test]
fn strict_array_origins_literal_nullable_and_nominal_compatibility() {
    let source = "function literal_nullable(?int $value){$items=[1,$value];$items[]=null;return count($items);} \
        class LiteralOriginBase{}class LiteralOriginChild extends LiteralOriginBase{} \
        $items=[new LiteralOriginBase(),new LiteralOriginChild()];$items[]=new LiteralOriginChild(); \
        echo literal_nullable(1);return count($items);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "3"); assert_eq!(returned, FakeValue::Int(3));
    }
}

/// A declared mixed array occupant remains a mixed element source for literals and first appends.
#[test]
fn strict_array_origins_mixed_array_occupants_keep_scalar_alternatives() {
    let source = "function mixed_array_occupant():mixed{return [1];} \
        $items=[mixed_array_occupant()];$items[]='allowed'; \
        $other=[];$other[]=mixed_array_occupant();$other[]=false;return count($items)+count($other);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(4));
    }
}
