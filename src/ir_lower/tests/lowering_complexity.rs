//! Purpose:
//! Pins the cost of lowering passes that scale with the closed world to the program size, so a
//! return to a quadratic walk fails a test instead of only slowing a framework build.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Counts work items with test-only counters rather than timing anything, so the bound is
//!   deterministic on any machine and under any load.

use super::lower_source;

/// `class C{i} extends Base { __construct(int $a = 1) }` for `count` classes, plus a dynamic
/// `new $c()` that makes lowering build one padding thunk per class (each thunk calls its own
/// class's constructor) and one interface-typed call site per class.
fn many_padded_constructors(count: usize) -> String {
    let mut source = String::from(
        "<?php\ninterface Runs { public function run(int $n): int; }\nabstract class Base implements Runs {}\n",
    );
    for index in 0..count {
        source.push_str(&format!(
            "final class C{index} extends Base {{\n    public function __construct(public int $a = 1) {{}}\n    public function run(int $n, int $extra = 0): int {{ return $n + $this->a + $extra; }}\n}}\nfunction call{index}(Runs $r): int {{ return $r->run({index}); }}\n"
        ));
    }
    source.push_str("$c = 'C' . ($argc - 1);\n$o = new $c();\necho call0($o), \"\\n\";\n");
    source
}

/// Candidates examined by `wider_override_classes` while lowering `source`.
fn wider_override_candidates(source: &str) -> u64 {
    crate::ir_lower::expr::WIDER_OVERRIDE_CANDIDATES.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::ir_lower::expr::WIDER_OVERRIDE_CANDIDATES.with(|count| count.get())
}

/// Every padding thunk calls its own class's constructor, and nearly every class declares one.
/// Walking every declarer per call made lowering quadratic in the class count (a third of
/// lowering the Symfony app); the examined candidates must stay linear in it.
#[test]
fn wider_override_dispatch_examines_a_linear_number_of_candidates() {
    let small = wider_override_candidates(&many_padded_constructors(40));
    let large = wider_override_candidates(&many_padded_constructors(160));
    assert!(small > 0, "the fixture no longer reaches the wider-override dispatch");
    // Four times the classes: linear work grows about fourfold, quadratic work sixteenfold.
    assert!(
        large <= small * 6,
        "wider-override candidates grew superlinearly: {small} for 40 classes, {large} for 160"
    );
}

/// `function f{i}(mixed $a)` comparing its untyped parameter `count` times: every comparison is
/// a numeric-domain question codegen answers on the fallback proof.
fn many_mixed_comparisons(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!("    if ($a < {index}) {{ $n++; }}\n"));
    }
    format!("<?php\nfunction f($a) {{\n    $n = 0;\n{body}    return $n;\n}}\necho f($argc), \"\\n\";\n")
}

/// (numeric fallback questions, numeric proofs built) while generating code for `source`.
fn numeric_fallback_work(source: &str) -> (u64, usize) {
    let module = lower_source(source);
    crate::codegen::NUMERIC_FALLBACK_QUERIES.with(|count| count.set(0));
    let before = crate::ir_passes::numeric_domain::proof_builds_for_tests();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    let builds = crate::ir_passes::numeric_domain::proof_builds_for_tests() - before;
    (crate::codegen::NUMERIC_FALLBACK_QUERIES.with(|count| count.get()), builds)
}

/// Codegen answers the numeric questions its frame proof left open on one proof per body:
/// building a fresh proof per comparison rescanned the function each time, quadratic in its
/// size. Sixteen times the comparisons must build the same number of proofs.
#[test]
fn codegen_numeric_fallback_builds_one_proof_per_body() {
    let (small_queries, small_builds) = numeric_fallback_work(&many_mixed_comparisons(4));
    let (large_queries, large_builds) = numeric_fallback_work(&many_mixed_comparisons(64));
    assert!(small_queries > 0, "the fixture no longer reaches the codegen numeric fallback");
    assert!(large_queries >= small_queries * 8, "{small_queries} vs {large_queries} questions");
    assert_eq!(small_builds, large_builds, "proofs grew with the comparisons: {small_builds} vs {large_builds}");
}

/// One body with `count` untyped (`Mixed`) locals, each released when the body returns.
fn many_mixed_locals(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!("    $v{index} = $a[{index}] ?? {index};\n    $n += is_int($v{index}) ? 1 : 0;\n"));
    }
    format!("<?php\nfunction f(array $a) {{\n    $n = 0;\n{body}    return $n;\n}}\necho f([$argc]), \"\\n\";\n")
}

/// Instructions scanned for a slot's stores while generating code for `source`.
fn local_store_scanned_instructions(source: &str) -> u64 {
    let module = lower_source(source);
    crate::codegen::LOCAL_STORE_SCANNED_INSTRUCTIONS.with(|count| count.set(0));
    crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    crate::codegen::LOCAL_STORE_SCANNED_INSTRUCTIONS.with(|count| count.get())
}

/// Each release of a `Mixed` local scanned the WHOLE body for the slot's stores: locals times
/// instructions, the top codegen frame of the Symfony Demo build. The scan must stay linear.
#[test]
fn mixed_local_release_scans_the_body_once() {
    let small = local_store_scanned_instructions(&many_mixed_locals(40));
    let large = local_store_scanned_instructions(&many_mixed_locals(160));
    assert!(small > 0, "the fixture no longer reaches the mixed local release scan");
    assert!(
        large <= small * 6,
        "the store scan grew superlinearly: {small} instructions for 40 locals, {large} for 160"
    );
}

/// A generator yielding `count` keyed pairs whose keys are untyped (`Mixed`) values.
fn many_keyed_yields(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!("    yield $k . '{index}' => $index + {index};\n"));
    }
    format!(
        "<?php\nfunction g($k) {{\n    $index = 0;\n{body}}}\nforeach (g($argv[0]) as $key => $value) {{ echo $key, '=', $value, \"\\n\"; }}\n"
    )
}

/// Whole-body use countings made while generating code for `source`.
fn codegen_use_countings(source: &str) -> u64 {
    let module = lower_source(source);
    crate::ir::VALUE_USE_COUNTINGS.with(|count| count.set(0));
    crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    crate::ir::VALUE_USE_COUNTINGS.with(|count| count.get())
}

/// Codegen counts a body's value uses once: each keyed `yield` asked whether its key box is
/// single-use and recounted the whole body to answer, quadratic in the generator's size.
#[test]
fn codegen_counts_value_uses_once_per_body() {
    let small = codegen_use_countings(&many_keyed_yields(4));
    let large = codegen_use_countings(&many_keyed_yields(64));
    assert_eq!(small, large, "use countings grew with the yields: {small} for 4, {large} for 64");
}

/// `count` statements of untyped (`Mixed`) division, modulo, comparison and `switch`: each one
/// asks whether some declared class has internal operator handlers.
fn many_mixed_operators(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!(
            "    $n += $a / ($b + {index}); $n += $a % ($b + {index}); if ($a < $b) {{ $n++; }}\n    switch ($a) {{ case {index}: $n++; break; }}\n"
        ));
    }
    format!("<?php\nfunction f($a, $b) {{\n    $n = 0;\n{body}    return $n;\n}}\necho f($argc, $argc), \"\\n\";\n")
}

/// Whole class-table operator scans made while checking and lowering `source`.
fn operator_class_scans(source: &str) -> u64 {
    crate::types::object_operators::OPERATOR_CLASS_SCANS.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::object_operators::OPERATOR_CLASS_SCANS.with(|count| count.get())
}

/// "Does a declared class have operator handlers?" is answered once per program by the checker
/// and once per lowering context: asking it at every binary operator scanned every declared
/// class each time, the top lowering frame on the Symfony build. Sixteen times the operators
/// must scan the class table the same number of times.
#[test]
fn operator_class_presence_is_scanned_once_not_per_operator() {
    let small = operator_class_scans(&many_mixed_operators(4));
    let large = operator_class_scans(&many_mixed_operators(64));
    assert!(small > 0, "the fixture no longer asks whether operator classes exist");
    assert_eq!(small, large, "operator-class scans grew with the operators: {small} vs {large}");
}

/// A program without an operator class has no plan to look for: every `mixed` operand used to
/// sort the whole class table to learn that, a tenth of the lowering of a Symfony build.
#[test]
fn mixed_operands_do_not_walk_the_class_table_without_operator_classes() {
    crate::types::object_operators::OPERATOR_PLAN_WALKS.with(|walks| walks.set(0));
    let _module = lower_source(&many_mixed_operators(8));
    let walks = crate::types::object_operators::OPERATOR_PLAN_WALKS.with(|walks| walks.get());
    assert_eq!(walks, 0, "operator lowering walked the class table {walks} times");
}

/// `count` classes, each writing a runtime-named property on `$this`: every class is a receiver
/// family the dynamic-storage pass resolves.
fn many_runtime_named_writers(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!(
            "class W{index} {{ public function set(string $name): void {{ $this->$name = {index}; }} }}\n"
        ));
    }
    source.push_str("(new W0())->set('p' . $argc);\necho \"ok\\n\";\n");
    source
}

/// `is_subclass_of` questions asked while checking and lowering `source`.
fn subclass_queries(source: &str) -> u64 {
    crate::types::checker::SUBCLASS_QUERIES.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::SUBCLASS_QUERIES.with(|count| count.get())
}

/// Each receiver class's family (the class and its subclasses) comes from a children index
/// built once, not from asking `is_subclass_of` of every class per receiver, which was
/// quadratic in the class count and the top checker frame on Symfony.
#[test]
fn receiver_families_do_not_ask_every_class_per_receiver() {
    let small = subclass_queries(&many_runtime_named_writers(40));
    let large = subclass_queries(&many_runtime_named_writers(160));
    // Four times the classes: linear work grows about fourfold, quadratic work sixteenfold.
    assert!(large <= small * 6 + 64, "subclass questions grew superlinearly: {small} for 40, {large} for 160");
}

/// One function with `count` runtime callable-array calls, plus classes they may reach.
fn many_callable_array_sites(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!("    $n += call_user_func([$o, $m], {index});\n"));
    }
    format!(
        "<?php\nclass P {{ public function a(int $x): int {{ return $x; }} public function b(int $x): int {{ return $x + 1; }} }}\n\
         class Q {{ public function a(int $x): int {{ return $x * 2; }} }}\n\
         function f($o, $m) {{\n    $n = 0;\n{body}    return $n;\n}}\n\
         echo f($argc > 1 ? new P() : new Q(), $argc > 2 ? 'b' : 'a'), \"\\n\";\n"
    )
}

/// Callable-array instance candidate lists built while generating code for `source`.
fn instance_target_builds(source: &str) -> u64 {
    let module = lower_source(source);
    crate::codegen::INSTANCE_TARGET_BUILDS.with(|count| count.set(0));
    crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    crate::codegen::INSTANCE_TARGET_BUILDS.with(|count| count.get())
}

/// Every runtime callable-array site of one class scope shares that scope's candidate list:
/// rebuilding it per site walked and recloned every emitted instance method each time.
#[test]
fn callable_array_sites_share_one_candidate_list_per_scope() {
    let small = instance_target_builds(&many_callable_array_sites(4));
    let large = instance_target_builds(&many_callable_array_sites(64));
    assert!(small > 0, "the fixture no longer reaches runtime callable-array dispatch");
    assert_eq!(small, large, "candidate lists grew with the sites: {small} vs {large}");
}

/// `class Base` with `count` methods, `count` subclasses each overriding one, and a function
/// calling every method on a `Base` receiver: `count` dispatch folds over Base's family.
fn many_base_dispatch_folds(count: usize) -> String {
    let mut source = String::from("<?php\nclass Base {\n");
    for index in 0..count {
        source.push_str(&format!("    public function m{index}(int $x): int {{ return $x; }}\n"));
    }
    source.push_str("}\n");
    for index in 0..count {
        source.push_str(&format!(
            "class C{index} extends Base {{ public function m{index}(int $x): int {{ return $x + {index}; }} }}\n"
        ));
    }
    source.push_str("function f(Base $b): int {\n    $n = 0;\n");
    for index in 0..count {
        source.push_str(&format!("    $n += $b->m{index}(1);\n"));
    }
    source.push_str("    return $n;\n}\necho f($argc > 1 ? new C0() : new Base()), \"\\n\";\n");
    source
}

/// `is_same_or_descendant_class` questions asked while lowering `source`.
fn descendant_queries(source: &str) -> u64 {
    crate::ir_lower::expr::DESCENDANT_QUERIES.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::ir_lower::expr::DESCENDANT_QUERIES.with(|count| count.get())
}

/// A dispatch fold over a base class ranges over the base's family, computed once per base:
/// each new (base, method) fold walked every class's parent chain, quadratic when a base's
/// many methods are each called.
#[test]
fn dispatch_folds_share_one_family_per_base_class() {
    let small = descendant_queries(&many_base_dispatch_folds(20));
    let large = descendant_queries(&many_base_dispatch_folds(80));
    // Four times the classes and methods: linear work grows about fourfold, quadratic sixteenfold.
    assert!(large <= small * 6 + 64, "descendant questions grew superlinearly: {small} for 20, {large} for 80");
}

/// `class Base` with `count` static methods and constants read through `static::` from one
/// method, and `count` subclasses each overriding one of each: late-static dispatch folds and
/// late-static constant candidates over Base's family.
fn many_late_static_folds(count: usize) -> String {
    let mut source = String::from("<?php\nclass Base {\n");
    for index in 0..count {
        source.push_str(&format!(
            "    const K{index} = {index};\n    public static function m{index}(int $x): int {{ return $x; }}\n"
        ));
    }
    source.push_str("    public static function all(): int {\n        $n = 0;\n");
    for index in 0..count {
        source.push_str(&format!("        $n += static::m{index}(1) + static::K{index};\n"));
    }
    source.push_str("        return $n;\n    }\n}\n");
    for index in 0..count {
        source.push_str(&format!(
            "class C{index} extends Base {{ const K{index} = {index} + 1; public static function m{index}(int $x): int {{ return $x + {index}; }} }}\n"
        ));
    }
    source.push_str("echo C0::all(), Base::all(), \"\\n\";\n");
    source
}

/// Late-static method folds and constant candidates share the base's family as well.
#[test]
fn late_static_folds_share_one_family_per_base_class() {
    let small = descendant_queries(&many_late_static_folds(20));
    let large = descendant_queries(&many_late_static_folds(80));
    assert!(large <= small * 6 + 64, "descendant questions grew superlinearly: {small} for 20, {large} for 80");
}

/// An interface-typed call copies its implementations' signatures only for the all-enum check
/// that reads them. Copying every implementer's signature at each of the `count` call sites was
/// quadratic in the class count and a top checker frame on the Symfony app.
#[test]
fn interface_calls_copy_no_class_implementation_signatures() {
    crate::types::checker::INTERFACE_IMPLEMENTATION_SIG_COPIES
        .with(|count| count.set(0));
    let _module = lower_source(&many_padded_constructors(40));
    let copies = crate::types::checker::INTERFACE_IMPLEMENTATION_SIG_COPIES
        .with(|count| count.get());
    assert_eq!(copies, 0, "class implementations of an interface must not be copied per call");
}

/// The positive control of the test above: an interface implemented only by enums still takes
/// the all-enum branch and copies exactly its implementations' signatures, once per call.
#[test]
fn interface_calls_copy_enum_implementation_signatures_for_the_enum_check() {
    let source = "<?php\ninterface Named { public function label(int $n): string; }\n\
        enum A: string implements Named { case X = 'x'; public function label(int $n): string { return $this->value . $n; } }\n\
        enum B: string implements Named { case Y = 'y'; public function label(int $n): string { return $this->value . $n; } }\n\
        function show(Named $named): string { return $named->label(1); }\n\
        echo show($argc > 1 ? A::X : B::Y), \"\\n\";\n";
    crate::types::checker::INTERFACE_IMPLEMENTATION_SIG_COPIES.with(|count| count.set(0));
    let _module = lower_source(source);
    let copies = crate::types::checker::INTERFACE_IMPLEMENTATION_SIG_COPIES.with(|count| count.get());
    assert!(copies >= 2, "the all-enum interface check copied {copies} signatures, expected both enums");
    assert_eq!(copies % 2, 0, "each checked call copies both enum implementations: {copies}");
}

/// `final class C{i} { __construct(public $v) }` and one `new C{i}(..)` each: every constructor
/// argument propagates its type into the classes that share the promoted property.
fn many_promoted_constructor_calls(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!(
            "final class C{index} {{ public function __construct(public $v) {{}} }}\n"
        ));
    }
    for index in 0..count {
        source.push_str(&format!("$o{index} = new C{index}({index});\n"));
    }
    source.push_str("echo $o0->v, \"\\n\";\n");
    source
}

/// Classes `propagate_constructor_arg_type` visited while checking `source`.
fn shared_property_classes_visited(source: &str) -> u64 {
    crate::types::checker::SHARED_PROPERTY_CLASSES_VISITED.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::SHARED_PROPERTY_CLASSES_VISITED.with(|count| count.get())
}

/// Each `new C(..)` argument used to scan every class of the program for the ones sharing the
/// promoted property (a hash probe per class): quadratic in the class count, and the dominant
/// cost of a method-pass round on the Symfony app. The visits must stay linear.
#[test]
fn constructor_argument_propagation_visits_a_linear_number_of_classes() {
    let small = shared_property_classes_visited(&many_promoted_constructor_calls(40));
    let large = shared_property_classes_visited(&many_promoted_constructor_calls(160));
    assert!(small > 0, "the fixture no longer reaches constructor argument propagation");
    assert!(
        large <= small * 6,
        "constructor-argument visits grew superlinearly: {small} for 40 classes, {large} for 160"
    );
}

/// Owner classes `subtype_dispatch_owners` examined while checking `source`.
fn subtype_dispatch_owners_examined(source: &str) -> u64 {
    crate::types::checker::SUBTYPE_DISPATCH_OWNERS_EXAMINED.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::SUBTYPE_DISPATCH_OWNERS_EXAMINED.with(|count| count.get())
}

/// Every interface-typed call site asked which of the method's declarers extend or implement the
/// receiver, walking each declarer's parent chain again: calls times declarers. The answer is
/// fixed per (receiver, method), so the examined owners must stay linear in the class count.
#[test]
fn subtype_dispatch_examines_a_linear_number_of_owners() {
    let small = subtype_dispatch_owners_examined(&many_padded_constructors(40));
    let large = subtype_dispatch_owners_examined(&many_padded_constructors(160));
    assert!(small > 0, "the fixture no longer reaches the subtype dispatch scan");
    assert!(
        large <= small * 6,
        "subtype-dispatch owners grew superlinearly: {small} for 40 classes, {large} for 160"
    );
}

/// `count` subclasses of `Base`, each declaring its own property, read through a `Base`
/// receiver: every read asks which descendants declare the property.
fn many_descendant_property_reads(count: usize) -> String {
    let mut source = String::from("<?php\nclass Base {}\n");
    for index in 0..count {
        source.push_str(&format!("final class D{index} extends Base {{ public int $p{index} = {index}; }}\n"));
    }
    source.push_str("function read(Base $b): int {\n    $sum = 0;\n");
    for index in 0..count {
        source.push_str(&format!("    if ($b instanceof D{index}) {{ $sum += 1; }} else {{ $sum += $b->p{index} ?? 0; }}\n"));
    }
    source.push_str("    return $sum;\n}\necho read(new D0()), \"\\n\";\n");
    source
}

/// Whole-table descendant scans made while checking `source`.
fn descendant_class_scans(source: &str) -> u64 {
    crate::types::checker::DESCENDANT_CLASS_SCANS.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::DESCENDANT_CLASS_SCANS.with(|count| count.get())
}

/// Every property read on a class receiver scanned (and sorted) the WHOLE class table for the
/// descendants declaring it: reads times classes, the top cost of the Symfony Demo's method
/// pass. The descendants of an ancestor are fixed, so the scans must not grow with the reads.
#[test]
fn descendant_property_lookup_scans_once_per_ancestor() {
    let small = descendant_class_scans(&many_descendant_property_reads(40));
    let large = descendant_class_scans(&many_descendant_property_reads(160));
    assert!(small > 0, "the fixture no longer reaches the descendant property lookup");
    assert!(
        large <= small * 2,
        "descendant scans grew with the property reads: {small} for 40 reads, {large} for 160"
    );
}

/// `count` implementations of one interface and `count` calls through an interface receiver.
fn many_interface_calls(count: usize) -> String {
    let mut source = String::from("<?php\ninterface I { function m(): int; }\n");
    for index in 0..count {
        source.push_str(&format!("final class C{index} implements I {{ function m(): int {{ return {index}; }} }}\n"));
    }
    source.push_str("function total(I $i): int {\n    $sum = 0;\n");
    for _ in 0..count {
        source.push_str("    $sum += $i->m();\n");
    }
    source.push_str("    return $sum;\n}\necho total(new C0()), \"\\n\";\n");
    source
}

/// Whole-table implementer scans made while checking `source`.
fn interface_implementer_scans(source: &str) -> u64 {
    crate::types::checker::INTERFACE_IMPLEMENTER_SCANS.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::INTERFACE_IMPLEMENTER_SCANS.with(|count| count.get())
}

/// Every call on an interface receiver collected and sorted the interface's implementers from
/// the WHOLE class table: calls times classes. They are fixed per interface.
#[test]
fn interface_call_collects_implementers_once_per_interface() {
    let small = interface_implementer_scans(&many_interface_calls(40));
    let large = interface_implementer_scans(&many_interface_calls(160));
    assert!(small > 0, "the fixture no longer reaches the interface implementer lookup");
    assert!(
        large <= small * 2,
        "implementer scans grew with the calls: {small} for 40 calls, {large} for 160"
    );
}

/// `count` classes and `count` writes of one property through a `mixed` receiver.
fn many_mixed_receiver_writes(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!("class C{index} {{ public $v = {index}; }}\n"));
    }
    source.push_str("function set(mixed $o): void {\n");
    for index in 0..count {
        source.push_str(&format!("    $o->v = {index};\n"));
    }
    source.push_str("}\n$c = new C0();\nset($c);\necho $c->v, \"\\n\";\n");
    source
}

/// Whole-table scans for mixed-receiver writes made while checking `source`.
fn mixed_receiver_mutation_scans(source: &str) -> u64 {
    crate::types::checker::MIXED_RECEIVER_MUTATION_SCANS.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::types::checker::MIXED_RECEIVER_MUTATION_SCANS.with(|count| count.get())
}

/// Every property write through a mixed receiver scanned the WHOLE class table for the classes
/// whose private slot the name shadows: writes times classes. The answer depends only on the
/// scope and the name.
#[test]
fn mixed_receiver_writes_scan_once_per_scope_and_name() {
    let small = mixed_receiver_mutation_scans(&many_mixed_receiver_writes(40));
    let large = mixed_receiver_mutation_scans(&many_mixed_receiver_writes(160));
    assert!(small > 0, "the fixture no longer reaches the mixed-receiver mutation scan");
    assert!(
        large <= small * 2,
        "mixed-receiver scans grew with the writes: {small} for 40 writes, {large} for 160"
    );
}

/// Classes whose methods pass instances of each other around, plus one `DateTime` use so the
/// date/time reference scan runs over every class-typed value.
fn many_typed_object_values(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        let next = (index + 1) % count;
        // An interface-typed value names a class-like the class table does not hold, which is
        // what sent each question through a case-insensitive scan of every class.
        source.push_str(&format!(
            "interface I{index} {{ public function id(): int; }}\nclass T{index} implements I{index} {{\n    public function id(): int {{ return {index}; }}\n    public function pass(T{next} $o, I{next} $c): int {{ return $c->id() + $o->id() + $c->id(); }}\n}}\n"
        ));
    }
    source.push_str("$d = new DateTime('2020-01-01');\necho $d->format('Y'), \"\\n\";\n");
    source
}

/// Parent-chain walks run by the date/time reference scan while lowering `source`.
fn datetime_chain_walks(source: &str) -> u64 {
    crate::ir_lower::builtin_datetime::DATETIME_CHAIN_WALKS.with(|count| count.set(0));
    let _module = lower_source(source);
    crate::ir_lower::builtin_datetime::DATETIME_CHAIN_WALKS.with(|count| count.get())
}

/// The date/time reference scan asks "is this a DateTime class?" for every class-typed value of
/// every function. Walking the parent chain per question -- and scanning every class for a name
/// the table does not hold verbatim -- grew with values times classes; the walks must stay linear
/// in the class count.
#[test]
fn datetime_descent_walks_a_linear_number_of_classes() {
    let small = datetime_chain_walks(&many_typed_object_values(40));
    let large = datetime_chain_walks(&many_typed_object_values(160));
    assert!(small > 0, "the fixture no longer reaches the date/time descent check");
    assert!(
        large <= small * 6,
        "date/time descent walks grew superlinearly: {small} for 40 classes, {large} for 160"
    );
}

/// `$mixed->has('k')` over `count` classes whose `has(string $k)` share one signature: every arm
/// converts the argument the same way and calls through the same vtable slot. The classes are
/// NOT final: a final class's arm calls its own body directly, so it differs from every other.
fn identical_has_ladder(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!(
            "class H{index} {{ public function has(string $k): bool {{ return $k === 'k{index}'; }} }}\n"
        ));
    }
    source.push_str("function probe(mixed $o): bool { return $o->has('k'); }\n");
    source.push_str("$c = 'H' . ($argc - 1);\necho probe(new $c()) ? 'y' : 'n';\n");
    source
}

/// Ladder arms of the program that are only a jump to an earlier, identical arm. Counted in the
/// emitted text: codegen may run on worker threads, where a thread-local counter is not seen.
fn merged_ladder_arms(source: &str) -> usize {
    let module = lower_source(source);
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    let lines: Vec<&str> = asm.lines().collect();
    lines
        .windows(2)
        .filter(|pair| {
            let arm = |line: &str| line.contains("_mixed_method_H");
            arm(pair[0])
                && pair[0].ends_with(':')
                && arm(pair[1])
                && matches!(pair[1].split_whitespace().next(), Some("b" | "jmp"))
        })
        .count()
}

/// A class ladder emits one arm per DISTINCT arm body: the Symfony build carried 145 copies of
/// the same `has()` arm in one helper, because each class got its own copy of class-independent
/// code. Every class beyond the first now shares the first arm.
#[test]
fn identical_mixed_method_arms_are_emitted_once() {
    let small = merged_ladder_arms(&identical_has_ladder(4));
    let large = merged_ladder_arms(&identical_has_ladder(40));
    assert_eq!(large - small, 36, "each extra class reuses the shared arm: {small} for 4, {large} for 40");
}

/// A suspended frame's owner visitor is an interned slot table plus a tail call into
/// `__rt_gc_frame_owners_table`, not one unrolled address/tag/call sequence per slot (1.22 M
/// instructions of the Symfony `--web` build). The table keeps the unrolled visitor's order and
/// descriptors: `inner()` owns the parameter's copy-on-write local, two graph locals guarded by a
/// ref-cell state word (`$param`, `$local`) and two raw reference cells (`$local` and `$alias`
/// once `&` binds them), whose descriptor carries the 128 raw-cell flag. Checked on the emitted
/// text because no current program reaches the walker: the collector does not yet traverse a
/// suspended coroutine caught in a cycle.
#[test]
fn suspended_frame_visitor_is_a_table_and_a_tail_call() {
    let source = r#"<?php
function inner(array $param) {
    $local = [$param];
    $alias = &$local;
    Fiber::suspend();
    echo count($alias), "\n";
}
$f = new Fiber(function () { inner([1]); });
$f->start();
$f->resume();
"#;
    let module = lower_source(source);
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).expect("codegen");
    let lines: Vec<&str> = asm.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.ends_with("inner__gc_frame_owners:"))
        .expect("inner() publishes an owner visitor");
    let body: Vec<&str> = lines[at + 1..]
        .iter()
        .take_while(|line| !line.ends_with(':') && !line.trim_start().starts_with(".globl"))
        .copied()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with([';', '#']) && !line.starts_with("//")
        })
        .collect();
    assert!(body.len() <= 3, "the visitor is a stub: {body:?}");
    let last = body.last().expect("stub body");
    assert!(last.contains("__rt_gc_frame_owners_table"), "the stub tail-calls the walker: {body:?}");
    let table = body
        .iter()
        .find_map(|line| line.split_whitespace().find(|word| word.starts_with("_data_")))
        .map(|word| word.trim_end_matches(',').split('@').next().unwrap().to_string())
        .expect("the stub names its table");
    let start = lines
        .iter()
        .position(|line| *line == format!("{table}:"))
        .expect("the table is emitted");
    let words: Vec<u64> = lines[start + 1..]
        .iter()
        .take_while(|line| line.trim_start().starts_with(".quad"))
        .map(|line| {
            let value = line.trim_start().trim_start_matches(".quad").trim();
            u64::from_str_radix(value.trim_start_matches("0x"), 16).expect("hex word")
        })
        .collect();
    let count = words[0] as usize;
    assert_eq!(words.len(), 1 + 3 * count, "count then one triple per owner: {words:?}");
    let entries: Vec<&[u64]> = words[1..].chunks(3).collect();
    let plain = entries.iter().filter(|entry| entry[1] == 0 && entry[2] == 0).count();
    let guarded = entries.iter().filter(|entry| entry[1] != 0 && entry[2] == 0).count();
    let raw_cells = entries.iter().filter(|entry| entry[1] == 0 && entry[2] & 128 != 0).count();
    // The parameter's copy-on-write local is a plain owner on top of the four above.
    assert_eq!((count, plain, guarded, raw_cells), (5, 1, 2, 2), "owner table entries: {entries:?}");
    assert!(entries.iter().all(|entry| entry[0] != 0), "every owner has a frame offset: {entries:?}");
}

/// `count` interface-typed calls and quiet reads of a mixed element.
fn many_interface_sites(count: usize) -> String {
    let mut source = String::from(
        "<?php\ninterface Runs { public function run(int $n): int; }\n\
         final class A implements Runs { public function run(int $n): int { return $n; } }\n\
         final class B implements Runs { public function run(int $n): int { return $n + 1; } }\n\
         final class Bag implements ArrayAccess {\n\
             public function offsetExists(mixed $o): bool { return false; }\n\
             public function offsetGet(mixed $o): mixed { return 1; }\n\
             public function offsetSet(mixed $o, mixed $v): void {}\n\
             public function offsetUnset(mixed $o): void {}\n}\n\
         function f(Runs $r, Bag $b, mixed $m, string $k): int {\n    $t = 0;\n",
    );
    for index in 0..count {
        source.push_str(&format!(
            "    $t += $r->run({index});\n    if (isset($m[$k])) {{ $t += {index}; }}\n    \
             $t += (int) $b[$k];\n"
        ));
    }
    source.push_str("    return $t;\n}\necho f(new A(), new Bag(), [], 'k'), \"\\n\";\n");
    source
}

/// Whole-table walks for class-against-interface questions while lowering `source`.
fn class_table_walks(source: &str) -> usize {
    use crate::ir_lower::expr::array_access_types::CLASS_TABLE_WALKS;
    CLASS_TABLE_WALKS.with(|walks| walks.set(0));
    let _module = lower_source(source);
    CLASS_TABLE_WALKS.with(|walks| walks.get())
}

/// These questions cover every class and come from each call site: a tenth of the lowering of a
/// Symfony build walked parents and interface trees per class per site.
#[test]
fn interface_questions_walk_the_class_table_once_not_per_site() {
    let small = class_table_walks(&many_interface_sites(4));
    let large = class_table_walks(&many_interface_sites(64));
    assert!(small > 0, "the fixture no longer asks a class table question");
    assert_eq!(small, large, "class table walks grew with the sites: {small} vs {large}");
}
