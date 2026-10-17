//! Purpose:
//! Structural coverage of invariant closure calls and conservative loop fallbacks.
//!
//! Called from:
//! - `cargo test --lib ir_lower::tests::loop_closures`.
//!
//! Key details:
//! - Tests inspect lowering before EIR optimization can inline or fold calls.
//! - Every supported target receives the same scalar call and identity gates.

use crate::ir::{Module, Op};

/// Counts closure calls in main separately from ordinary named helper calls.
fn calls(module: &Module) -> (usize, usize) {
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    let direct = main.instructions.iter().filter(|inst| inst.op == Op::Call).count();
    let descriptor = main.instructions.iter()
        .filter(|inst| inst.op == Op::CallableDescriptorInvoke).count();
    (direct, descriptor)
}

/// Builds a dynamic-count loop around one caller-provided body and update.
fn fixture(closure: &str, body: &str, update: &str) -> String {
    format!("<?php $n=$argc; $f={closure}; $sum=0; for($i=0;$i<$n;{update}){{{body}}} echo $sum;")
}

/// Scalar closure identities survive for/while lowering on every target.
#[test]
fn known_loop_closure_scalar_calls_on_all_targets() {
    for source in [
        fixture("function($x){return $x+1;}", "$sum+=$f(1);", "$i++"),
        "<?php $n=$argc; $f=function($x){return $x+1;}; $sum=0;$i=0; while($i<$n){$sum+=$f(1);$i++;} echo $sum;".to_string(),
    ] {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (1, 0), "{target}");
        }
    }
}

/// Calls requiring return validation, references, variadics, or non-scalar captures stay generic.
#[test]
fn known_loop_closure_rejects_fallible_or_non_scalar_targets() {
    for source in [
        fixture("function(int $x):int{return $x+1;}", "$sum+=$f(1);", "$i++"),
        fixture("function($x){throw new RuntimeException('stop');}", "$sum+=$f(1);", "$i++"),
        fixture("function(&$x){return $x;}", "$sum+=$f($n);", "$i++"),
        fixture("function(...$xs){return 1;}", "$sum+=$f(1);", "$i++"),
        "<?php $word='x'; $f=function($x)use($word){return $x+1;};$sum=0;for($i=0;$i<$argc;$i++){$sum+=$f(1);}echo$sum;".to_string(),
        "<?php $k=0; $f=function($x)use(&$k){$k++;return $x;};$sum=0;for($i=0;$i<$argc;$i++){$sum+=$f(1);}echo$sum;".to_string(),
        "<?php function nestedInc($x){return $x+1;} $f=function($x){return nestedInc($x);};$sum=0;for($i=0;$i<$argc;$i++){$sum+=$f(1);}echo$sum;".to_string(),
    ] {
        let module = super::lower_source(&source);
        assert_eq!(calls(&module), (0, 1), "{source}");
    }
}

/// Coercion, expression arguments, named/spread arguments and omitted defaults keep the binder.
#[test]
fn known_loop_closure_rejects_non_exact_arguments() {
    for source in [
        fixture("function(float $x):float{return $x;}", "$sum+=$f(1);", "$i++"),
        fixture("function($x){return $x;}", "$sum+=$f($i+1);", "$i++"),
        fixture("function($x){return $x;}", "$sum+=$f(x:$i);", "$i++"),
        fixture("function($x){return $x;}", "$sum+=$f(...[$i]);", "$i++"),
        fixture("function($x=1){return $x;}", "$sum+=$f();", "$i++"),
    ] {
        assert_eq!(calls(&super::lower_source(&source)), (0, 1), "{source}");
    }
}

/// Reassignment on any backedge invalidates the entry target before the first emitted call.
#[test]
fn known_loop_closure_rejects_every_backedge_writer() {
    for body in [
        "$sum+=$f(1);$f=$g;",
        "$sum+=$f(1);if($i==0){$f=$g;continue;}",
        "$sum+=$f(1);for($j=0;$j<1;$j++){$f=$g;}",
        "$sum+=$f(1);$alias=&$f;",
        "$sum+=$f(1);eval('$f=null;');",
    ] {
        let source = format!("<?php $n=$argc;$f=function($x){{return $x+1;}};$g=function($x){{return $x+10;}};$sum=0;for($i=0;$i<$n;$i++){{{body}}}echo$sum;");
        assert_eq!(calls(&super::lower_source(&source)), (0, 1), "{source}");
    }
    let source = "<?php $f=function($x){return $x+1;};$g=function($x){return $x+10;};$sum=0;for($i=0;$i<$argc;$f=$g){$sum+=$f(1);$i++;}echo$sum;";
    assert_eq!(calls(&super::lower_source(source)), (0, 1));
}

/// A do/while reassignment must discard the stale target, while stable legacy calls remain direct.
#[test]
fn known_loop_closure_checks_do_while_identity_before_emission() {
    let source = "<?php $f=function($x){return $x+1;};$g=function($x){return $x+10;};$sum=0;$i=0;do{$sum+=$f(1);if($i==0){$f=$g;}$i++;}while($i<$argc);echo$sum;";
    assert_eq!(calls(&super::lower_source(source)), (0, 1));
    let stable = "<?php $f=function(int $x):int{return $x+1;};$sum=0;$i=0;do{$sum+=$f(1);$i++;}while($i<$argc);echo$sum;";
    assert_eq!(calls(&super::lower_source(stable)), (1, 0));
}

/// Typed identity returns and by-value scalar captures use exact scalar ABI operands.
#[test]
fn known_loop_closure_preserves_scalar_capture_snapshots() {
    let source = "<?php $k=3;$f=function($x)use($k){return $x+$k;};$k=100;$sum=0;for($i=0;$i<$argc;$i++){$sum+=$f(1);}echo$sum;";
    assert_eq!(calls(&super::lower_source(source)), (1, 0));
    let typed = fixture("function(int $x):int{return $x;}", "$sum+=$f(1);", "$i++");
    assert_eq!(calls(&super::lower_source(&typed)), (1, 0));
}

/// A condition-created closure must not bypass the invariant-entry gates in the body.
#[test]
fn known_loop_closure_discards_fresh_condition_bindings() {
    let source = "<?php $sum=0;for($i=0;($f=function($x){return $x+1;}) && $i<$argc;$i++){$sum+=$f(1);}echo$sum;";
    assert_eq!(calls(&super::lower_source(source)), (0, 1));
}

/// Existing named-callable condition lowering keeps its original direct path.
#[test]
fn known_loop_closure_preserves_non_closure_header_facts() {
    let source = "<?php function checkIteration(int $i):bool{return $i<2;} $predicate=checkIteration(...);for($i=0;$predicate($i);$i++){echo$i;}";
    assert_eq!(calls(&super::lower_source(source)), (1, 0));
}

/// Mixed arithmetic cannot hide string producers or unknown global input behind deopt effects.
#[test]
fn known_loop_closure_rejects_non_scalar_body_state() {
    for source in [
        fixture("function($x){$word='abc';return $x+$word;}", "$sum+=$f(1);", "$i++"),
        "<?php $input=1; $f=function($x){global $input;return $input+$x;};$sum=0;for($i=0;$i<$argc;$i++){$sum+=$f(1);}echo$sum;".to_string(),
    ] {
        assert_eq!(calls(&super::lower_source(&source)), (0, 1), "{source}");
    }
}

/// Opaque calls, reference aliases, catch bindings and indirect writers discard entry facts.
#[test]
fn known_loop_closure_rejects_indirect_identity_hazards() {
    for (prefix, body) in [
        ("$alias=&$f;", "$sum+=$f(1);"),
        ("function poke(&$value):void{$value=function($x){return $x+10;};}", "$sum+=$f(1);poke($f);"),
        ("function poke():void{global $f;$f=null;}", "$sum+=$f(1);poke();"),
        ("", "$sum+=$f(1);foreach([1] as $v){$f=$g;}"),
        ("", "try{$sum+=$f(1);}catch(Exception $f){}"),
        ("", "$sum+=$f(1);function declaredInside():int{return 1;}"),
    ] {
        let source = format!("<?php $f=function($x){{return $x+1;}};$g=function($x){{return $x+10;}};{prefix}$sum=0;for($i=0;$i<$argc;$i++){{{body}}}echo$sum;");
        assert_eq!(calls(&super::lower_source(&source)).1, 1, "{source}");
    }
}

/// Typed scalar identity arguments and returns use the existing direct ABI on every target.
#[test]
fn known_loop_closure_exact_scalar_signatures_on_all_targets() {
    for (signature, result_type, argument) in [("int", "int", "1"), ("float", "float", "1.5"),
        ("bool", "bool", "true"), ("false", "false", "false"),
        ("int", "float", "1")]
    {
        let source = fixture(&format!("function({signature} $x):{result_type}{{return $x;}}"),
            &format!("$result=$f({argument});"), "$i++");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (1, 0), "{target}: {signature}");
        }
    }
}

/// A pure nested static call must stay generic even after its effect summary is refined.
#[test]
fn known_loop_closure_rejects_refined_static_calls_on_all_targets() {
    let source = "<?php class Step{public static function inc($x){return$x+1;}}$f=function($x){return Step::inc($x);};$s=0;for($i=0;$i<$argc;$i++){$s+=$f(1);}echo$s;";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        assert_eq!(calls(&module), (0, 1), "{target}");
    }
}

/// Branching bodies and explicit casts remain outside the initial primitive-body proof.
#[test]
fn known_loop_closure_rejects_unproved_body_shapes() {
    for source in [
        fixture("function($x){if($x>0){return$x;}return 0;}", "$sum+=$f(1);", "$i++"),
        fixture("function($x):int{return (int)($x+1);}", "$sum+=$f(1);", "$i++"),
    ] {
        assert_eq!(calls(&super::lower_source(&source)), (0, 1), "{source}");
    }
}

/// A proved int|float backedge uses a direct Mixed call. The counter is not specialized to I64.
#[test]
fn known_loop_closure_admits_proved_numeric_arguments_on_all_targets() {
    for loop_body in [
        "for($i=0;$i<2;$i++){$out=$f($value);$value=1.5;}",
        "$i=0;while($i<2){$out=$f($value);$value=1.5;$i++;}",
        "for($i=0;$i<2;$value=1.5){$out=$f($value);$i++;}",
        "for($i=0;$i<2;$i++){$out=$f($value);($value=1.5);}",
        "for($i=0;$i<2;$i++){$out=$f($i);}",
        "$i=0;while($i<2){$out=$f($i);$i++;}",
        "for($i=0;$i<$argc+1;$i++){$out=$f($value);$value++;}",
        "$i=0;while($i<$argc+1){$out=$f($value);$value++;$i++;}",
    ] {
        let source = format!("<?php $f=function($x){{return $x+1;}};$value=10;$out=0;{loop_body}echo$out;");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (1, 0), "{target}: {source}");
            assert_mixed_direct_call(&module, &source);
        }
    }
}

/// Unproved writes, complex arguments, and unsupported control flow keep the descriptor invoke.
#[test]
fn known_loop_closure_rejects_argument_writes_on_all_targets() {
    for loop_body in [
        "for($i=0;$i<2;$i++){$out=$f($value);$value=null;}",
        "$i=0;while($i<2){$out=$f($value);$value=null;$i++;}",
        "$i=0;do{$out=$f($value);$value=1.5;$i++;}while($i<2);",
        "$i=0;do{$out=$f($value+0);$value=1.5;$i++;}while($i<2);",
        "$i=0;while($i++<2 && ($value=($i==1?10:1.5))>=0){$out=$f($value);}",
        "for($i=0;$i<2;$i++){$out=$f($value);if($i==0){$value=null;continue;}}",
        "for($i=0;$i<2;$i++){$out=$f($value);for($j=0;$j<1;$j++){$value=1.5;}}",
        "for($i=0;$i<2;$i++){$out=$f($value+0);$value=1.5;}",
    ] {
        let source = format!("<?php $f=function($x){{return $x+1;}};$value=10;$out=0;{loop_body}echo$out;");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (0, 1), "{target}: {source}");
        }
    }
}

/// An admitted Mixed call passes the boxed local and does not narrow it back to int.
fn assert_mixed_direct_call(module: &Module, source: &str) {
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    assert!(main.instructions.iter().all(|inst| inst.op != Op::MixedCastInt), "{source}");
    // `$argc + 1` is still a checked int add. The admitted argument itself is a Mixed load.
    let call = main.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
    let operand = call.operands.first().copied().unwrap();
    let defining = main.instructions.iter().find(|inst| inst.result == Some(operand)).unwrap();
    assert_eq!(defining.op, Op::LoadLocal, "{source}");
    assert_eq!(defining.result_php_type, crate::types::PhpType::Mixed, "{source}");
}

/// A Mixed value returned from another call is not a numeric seed, so the closure stays generic.
#[test]
fn known_loop_closure_rejects_unproved_mixed_arguments() {
    let source = "<?php function box($value): mixed { return $value; } $f=function($x){return $x+1;}; $value=box(10); $out=0; for($i=0;$i<2;$i++){$out=$f($value); $value='x';} echo $out;";
    assert_eq!(calls(&super::lower_source(source)), (1, 1), "{source}");
}

/// Literal and unwritten scalar argument storage remains eligible for direct calls.
#[test]
fn known_loop_closure_accepts_unwritten_scalar_arguments_on_all_targets() {
    for argument in ["1", "$value"] {
        let source = format!("<?php $value=$argc;$f=function($x){{return$x+1;}};$sum=0;for($i=0;$i<$argc;$i++){{$sum+=$f({argument});}}echo$sum;");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (1, 0), "{target}: {argument}");
        }
    }
}

/// One admitted numeric call cannot authorize another call whose argument changes tag.
#[test]
fn known_loop_closure_numeric_proof_is_scoped_to_call_sites() {
    for replacement in ["null", "true", "false"] {
        for body in [
            format!("$result=$f($value);$value={replacement};$sink=$f($i);"),
            format!("$sink=$f($i);$result=$f($value);$value={replacement};"),
        ] {
            for loop_body in [
                format!("for($i=0;$i<2;$i++){{{body}}}"),
                format!("$i=0;while($i<2){{{body}$i++;}}"),
            ] {
                let source = format!("<?php $f=function($x){{return$x;}};$value=10;{loop_body}echo gettype($result),':',$result;");
                for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                    let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
                    assert_eq!(calls(&module), (1, 1), "{target}: {source}");
                }
            }
        }
    }
}

/// Numeric bindings require a proof even for other calls with literal scalar arguments.
#[test]
fn known_loop_closure_numeric_proof_keeps_non_numeric_literals_generic() {
    for argument in ["null", "true"] {
        let source = fixture("function($x){return$x;}",
            &format!("$result=$f({argument});$sink=$f($i);"), "$i++");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (1, 1), "{target}: {argument}");
        }
    }
}

/// Each separately proved numeric call still uses the direct Mixed ABI in the same loop.
#[test]
fn known_loop_closure_numeric_proof_admits_multiple_numeric_sites() {
    let source = fixture("function($x){return$x+1;}", "$sum+=$f($i);$sum+=$f($i);", "$i++");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        assert_eq!(calls(&module), (2, 0), "{target}");
        assert_mixed_direct_call(&module, &source);
    }
}

/// A literal backtrace callable requests frame metadata without adding a call in the loop.
fn transport_fixture(source: &str) -> String {
    format!("<?php $transportBacktraceName='debug_backtrace'; {source}")
}

/// Public numeric arguments and captures stay distinct from the hidden argument-array transport.
#[test]
fn known_loop_closure_numeric_transport_on_all_targets() {
    for (capture, body) in [
        ("3", "for($i=0;$i<$argc+1;$i++){$sum+=$f($i,1);}"),
        ("3", "$i=0;while($i<$argc+1){$sum+=$f($i,1);$i++;}"),
        ("1.5", "for($i=0;$i<$argc+1;$i++){$sum+=$f($i,1);}"),
    ] {
        let source = transport_fixture(&format!(
            "$k={capture};$f=function($x,$y)use($k){{return$x+$y+$k;}};$sum=0;{body}echo$sum;"));
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert!(module.closures.iter()
                .any(|function| function.params.iter().any(|param| param.name == crate::func_args::HIDDEN_ARGS_PARAM)),
                "{target}: fixture must contain the compiler-only transport");
            if capture == "1.5" {
                // The existing scalar-body proof refuses the float-unboxing operation.
                assert_eq!(calls(&module), (0, 1), "{target}: float capture fallback");
                continue;
            }
            assert_eq!(calls(&module), (1, 0), "{target}: {source}");
            let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
            assert!(main.instructions.iter().all(|inst| inst.op != Op::MixedCastInt), "{target}");
            let call = main.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
            assert_eq!(call.operands.len(), 4, "{target}: two arguments, transport and capture");
            assert_eq!(main.value(call.operands[0]).unwrap().ir_type,
                crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed), "{target}");
            assert_eq!(main.value(call.operands[2]).unwrap().ir_type,
                crate::ir::IrType::Heap(crate::ir::IrHeapKind::Array), "{target}");
            assert_eq!(main.value(call.operands[3]).unwrap().ir_type, crate::ir::IrType::I64, "{target}");
        }
    }
}

/// Internal metadata cannot broaden admission from a numeric site to a null or bool site.
#[test]
fn known_loop_closure_transport_preserves_per_call_numeric_seals() {
    for value in ["null", "true"] {
        for body in [
            format!("for($i=0;$i<$argc+1;$i++){{$out=$f($value);$value={value};$sink=$f($i);}}"),
            format!("$i=0;while($i<$argc+1){{$out=$f($value);$value={value};$sink=$f($i);$i++;}}"),
        ] {
            let source = transport_fixture(&format!(
                "$f=function($x){{return$x;}};$value=10;$out=0;$sink=0;{body}echo$out;"));
            for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                    std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
                assert_eq!(calls(&module), (1, 1), "{target}: {source}");
            }
        }
    }
}

/// Introspection reads, user variadics and surplus source arguments retain the descriptor binder.
#[test]
fn known_loop_closure_transport_rejects_public_variadics_and_reads() {
    for (closure, args) in [
        ("function($x){return func_num_args();}", "$i"),
        ("function($x){return func_get_arg(0);}", "$i"),
        ("function(...$__elephc_func_args){return 1;}", "$i"),
        ("function($x){return$x;}", "$i,1"),
        ("function(float $x):float{return$x;}", "1"),
    ] {
        let source = transport_fixture(&format!(
            "$f={closure};$out=0;for($i=0;$i<$argc+1;$i++){{$out=$f({args});}}echo$out;"));
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
            assert_eq!(calls(&module), (0, 1), "{target}: {source}");
        }
    }
}
