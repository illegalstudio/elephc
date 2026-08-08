//! Purpose:
//! Pins the reachability rules that decide whether an injected prelude declaration survives.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The cases are grouped by the RISK each one covers, not by the code path they exercise. A
//!   pruner is only as good as the shapes it refuses to over-prune, and every case here is a way
//!   a real program reaches a symbol without calling it by name.

use super::*;
use crate::parser::ast::Program;

/// Parses user-facing PHP.
fn parse(source: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
    crate::parser::parse(&tokens).expect("test source must parse")
}

/// The image prelude as a program reaching it would see it.
fn pruned_image(source: &str) -> Program {
    let program = parse(source);
    let roots = collect_roots(&program);
    prune(crate::image_prelude::image_declarations(), &roots)
}

/// Returns whether a declaration of `name` survived.
fn kept(program: &Program, name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. }
        | StmtKind::ClassDecl { name: declared, .. } => {
            declared.eq_ignore_ascii_case(name)
        }
        _ => false,
    })
}

/// Counts the declarations this pass can drop, so a test can talk about the surface's size.
fn prunable(program: &Program) -> usize {
    program
        .iter()
        .filter(|stmt| declared_symbol(stmt).is_some())
        .count()
}

/// THE MEASURED CASE. A GD-only program must not carry Imagick, Gmagick or Cairo — that is the
/// 4.35 MB of assembly this whole pass exists to remove.
#[test]
fn gd_only_program_drops_the_other_extension_surfaces() {
    let pruned = pruned_image("<?php $im = imagecreatetruecolor(4, 4); imagedestroy($im);");

    assert!(kept(&pruned, "imagecreatetruecolor"));
    assert!(kept(&pruned, "imagedestroy"));

    for absent in [
        "Imagick",
        "ImagickDraw",
        "ImagickPixel",
        "Gmagick",
        "GmagickDraw",
        "CairoContext",
        "CairoImageSurface",
    ] {
        assert!(
            !kept(&pruned, absent),
            "{absent} is unreachable from two GD calls and must not survive"
        );
    }

    let full = prunable(&crate::image_prelude::image_declarations());
    let left = prunable(&pruned);
    assert!(
        left * 4 < full,
        "a two-call GD program should keep well under a quarter of the surface, kept {left} of {full}"
    );
}

/// A class named by `new` roots the class AND ALL ITS METHODS. Method-level pruning is not
/// attempted, so this is the granularity contract.
#[test]
fn instantiating_a_class_keeps_it_whole() {
    let pruned = pruned_image("<?php $i = new Imagick(); $i->readImage('x.png');");
    assert!(kept(&pruned, "Imagick"));
    let methods = pruned
        .iter()
        .find_map(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } if name == "Imagick" => Some(methods.len()),
            _ => None,
        })
        .expect("Imagick survived");
    assert!(
        methods > 300,
        "rooting a class keeps every method, found only {methods}"
    );
}

/// A method call on an untyped receiver names no class, so every class declaring that method is
/// rooted. This is the deliberate over-approximation — the test exists so a future change that
/// narrows it has to say so out loud.
#[test]
fn a_method_call_roots_every_class_declaring_it() {
    // `clear()` is declared by seven classes in the image surface.
    let pruned = pruned_image("<?php function f($x) { $x->clear(); }");
    assert!(kept(&pruned, "Imagick"));
    assert!(kept(&pruned, "ImagickDraw"));
    assert!(kept(&pruned, "GmagickDraw"));
}

/// THE SILENT FAILURE MODE. A probe is not a call, and pruning its subject would flip the guard
/// to its else branch with no diagnostic at all.
#[test]
fn a_function_exists_probe_keeps_its_subject() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         if (function_exists('imagecreatefromwebp')) { echo 1; }",
    );
    assert!(
        kept(&pruned, "imagecreatefromwebp"),
        "a probed function must survive or the guard silently changes branch"
    );
}

/// The same rule for classes.
#[test]
fn a_class_exists_probe_keeps_its_subject() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         if (class_exists('Imagick')) { echo 1; }",
    );
    assert!(kept(&pruned, "Imagick"));
}

/// A literal callback is a reference even though nothing calls it syntactically.
#[test]
fn a_literal_callback_keeps_its_target() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         call_user_func('imagecolorallocate', null, 1, 2, 3);",
    );
    assert!(kept(&pruned, "imagecolorallocate"));
}

/// THE FALLBACK THAT REPLACES SURRENDER. An unanalysable dynamic call does not restore the whole
/// surface; it roots the symbols the program NAMES.
#[test]
fn a_dynamic_call_harvests_literals_instead_of_keeping_everything() {
    let pruned = pruned_image(
        "<?php $f = 'imagecreatefrompng'; $im = $f('x.png'); imagedestroy($im);",
    );
    assert!(
        kept(&pruned, "imagecreatefrompng"),
        "the dispatched name appears as a literal and must be rooted"
    );
    assert!(
        !kept(&pruned, "Imagick"),
        "a dynamic call must not resurrect the surfaces the program never names"
    );
}

/// True introspection names nothing, so no root set approximates it and the surface is kept.
#[test]
fn introspection_keeps_the_whole_surface() {
    let pruned = pruned_image("<?php imagedestroy(imagecreatetruecolor(1, 1)); $all = get_defined_functions();");
    assert_eq!(
        prunable(&pruned),
        prunable(&crate::image_prelude::image_declarations()),
        "enumerating the symbol table must disable pruning entirely"
    );
}

/// `eval` can name anything at runtime, so it counts as introspection.
#[test]
fn eval_keeps_the_whole_surface() {
    let pruned = pruned_image("<?php imagedestroy(imagecreatetruecolor(1, 1)); eval('$x = 1;');");
    assert_eq!(
        prunable(&pruned),
        prunable(&crate::image_prelude::image_declarations())
    );
}

/// THE CLOSURE. A kept declaration's own references are roots in turn — including through a type
/// hint, which is how a method reaches a class it never instantiates.
#[test]
fn transitive_references_survive() {
    let pruned = pruned_image("<?php $i = new Imagick();");
    assert!(kept(&pruned, "Imagick"));
    assert!(
        kept(&pruned, "ImagickException"),
        "Imagick throws it, so it must come along"
    );
}

/// Constants and externs are never candidates: they cost nothing downstream and dropping them
/// would trade a measured zero for a real risk.
#[test]
fn constants_and_externs_are_never_dropped() {
    let full = crate::image_prelude::image_declarations();
    let pruned = pruned_image("<?php echo 1;");
    let count = |program: &Program, f: fn(&Stmt) -> bool| program.iter().filter(|s| f(s)).count();
    let is_const = |stmt: &Stmt| matches!(stmt.kind, StmtKind::ConstDecl { .. });
    let is_extern = |stmt: &Stmt| matches!(stmt.kind, StmtKind::ExternFunctionDecl { .. });
    assert_eq!(count(&full, is_const), count(&pruned, is_const));
    assert_eq!(count(&full, is_extern), count(&pruned, is_extern));
}

/// The pass only ever filters the PRELUDE. A user declaration reaching it would be a bug, so the
/// contract is pinned here even though `inject_if_used` never passes one in.
#[test]
fn user_declarations_are_not_this_passs_to_remove() {
    let user = parse("<?php function my_helper() { return 1; } echo my_helper();");
    let roots = collect_roots(&user);
    let pruned = prune(user.clone(), &roots);
    assert!(
        kept(&pruned, "my_helper"),
        "a program's own reachable declaration must survive"
    );
}

/// A DISPATCH HAZARD INSIDE THE PRELUDE counts as one, not just a hazard in the program.
///
/// `__ElephcCallableSessionHandler::read` does `call_user_func($this->readCb, …)`. Keeping that
/// class puts a call this walk cannot name into the surface, exactly as if the user had written
/// one — so the literal harvest has to switch on. The pruner this replaced checked the same
/// thing by surrendering; missing it entirely is what a rewrite is most likely to lose, so it is
/// pinned here rather than left to the day a prelude dispatches to one of its own helpers.
#[test]
fn a_dynamic_call_inside_a_kept_declaration_switches_on_harvesting() {
    let prelude = crate::synthetic_class::internal_declarations(|| {
        vec![
            // Reachable directly, and it dispatches on a value this walk cannot follow.
            crate::synthetic_class::function("entry")
                .body(vec![crate::synthetic_class::s_expr(
                    crate::synthetic_class::e_call(
                        "call_user_func",
                        vec![crate::synthetic_class::e_var("cb")],
                    ),
                )])
                .build(),
            // Named nowhere but by a literal in the program.
            crate::synthetic_class::function("harvested")
                .returning(crate::synthetic_class::e_int(1))
                .build(),
            // Named nowhere at all.
            crate::synthetic_class::function("unreachable")
                .returning(crate::synthetic_class::e_int(2))
                .build(),
        ]
    });

    let program = parse("<?php entry(); $name = 'harvested';");
    let roots = collect_roots(&program);
    let pruned = prune(prelude, &roots);

    assert!(kept(&pruned, "entry"));
    assert!(
        kept(&pruned, "harvested"),
        "the prelude's own dynamic call must widen the roots to what the program names"
    );
    assert!(
        !kept(&pruned, "unreachable"),
        "widening is to the names in play, not to everything"
    );
}

/// The literals seen BEFORE the dynamic call count too, because the walk order is an accident of
/// declaration order and the answer must not depend on it.
#[test]
fn harvesting_reaches_back_to_literals_seen_earlier() {
    let prelude = crate::synthetic_class::internal_declarations(|| {
        vec![
            crate::synthetic_class::function("dispatcher")
                .body(vec![crate::synthetic_class::s_expr(
                    crate::synthetic_class::e_call(
                        "call_user_func",
                        vec![crate::synthetic_class::e_var("cb")],
                    ),
                )])
                .build(),
            crate::synthetic_class::function("named_early")
                .returning(crate::synthetic_class::e_int(1))
                .build(),
        ]
    });

    // The literal is read before the call that makes it a root.
    let program = parse("<?php $which = 'named_early'; dispatcher();");
    let roots = collect_roots(&program);
    let pruned = prune(prelude, &roots);
    assert!(kept(&pruned, "named_early"));
}

// The cases below came out of a code review that asked one question three ways: what turns a
// STRING into a symbol, and does this walk know about all of those channels? Each is a way a
// real program reaches a declaration without ever calling it by name.

/// A CALLABLE-TAKING BUILTIN names its target plainly. `register_shutdown_function('x')` contains
/// no dynamic call at all, so nothing would switch harvesting on — the reference has to count on
/// its own, or the handler goes silently uninstalled.
#[test]
fn a_string_callback_builtin_names_its_target() {
    for source in [
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); register_shutdown_function('imagecolorallocate');",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); set_error_handler('imagecolorallocate');",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); usort($a, 'imagecolorallocate');",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); $m = array_map('imagecolorallocate', $a);",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); ob_start('imagecolorallocate');",
    ] {
        assert!(
            kept(&pruned_image(source), "imagecolorallocate"),
            "a literal callable is a reference: {source}"
        );
    }
}

/// The `'Class::method'` and `[Class, method]` callable forms name a class, not a function.
#[test]
fn the_qualified_callable_forms_name_their_class() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         call_user_func('Imagick::clear');",
    );
    assert!(kept(&pruned, "Imagick"), "'Class::method' names the class");

    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         call_user_func(['ImagickDraw', 'clear']);",
    );
    assert!(kept(&pruned, "ImagickDraw"), "the array form names it too");
}

/// A PROBE ON A COMPUTED NAME cannot be approximated: nothing in the program says which symbol it
/// asks about, and a wrong answer is silent — the guard simply takes its else branch. So it keeps
/// the surface, unlike a dynamic CALL, which fails loudly and only widens the roots.
#[test]
fn a_probe_on_a_computed_name_keeps_the_whole_surface() {
    let pruned = pruned_image(
        "<?php $fn = 'imagecreatefrom' . $format;
         if (function_exists($fn)) { $im = $fn($path); }",
    );
    assert_eq!(
        prunable(&pruned),
        prunable(&crate::image_prelude::image_declarations()),
        "a probe this walk cannot read must not silently answer false"
    );
}

/// The probe family is bigger than the three the first cut named.
#[test]
fn every_probe_keeps_its_subject() {
    for source in [
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); var_dump(method_exists('Imagick', 'clear'));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); var_dump(property_exists('Imagick', 'x'));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); print_r(get_class_methods('Imagick'));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); print_r(class_implements('Imagick'));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); print_r(class_parents('Imagick'));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); print_r(class_uses('Imagick'));",
    ] {
        assert!(
            kept(&pruned_image(source), "Imagick"),
            "a probed class must survive: {source}"
        );
    }
}

/// `is_a` AND `is_subclass_of` NAME THEIR CLASS SECOND — the first argument is the subject.
///
/// Both spellings are checked with a LITERAL subject, so the class argument is genuinely the one
/// being read. With `$o` there instead, the walk cannot read the subject, gives up and keeps the
/// whole surface, and the assertion below would hold no matter which argument the table looked
/// at — a test that cannot fail for the reason it names.
#[test]
fn the_subclass_probes_name_their_class_second() {
    for source in [
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); var_dump(is_a('Gmagick', 'Imagick', true));",
        "<?php imagedestroy(imagecreatetruecolor(1, 1)); var_dump(is_subclass_of('Gmagick', 'Imagick'));",
    ] {
        let pruned = pruned_image(source);
        assert!(
            prunable(&pruned) < prunable(&crate::image_prelude::image_declarations()),
            "a readable probe must not fall back to keeping everything: {source}"
        );
        assert!(kept(&pruned, "Imagick"), "the class argument: {source}");
        assert!(kept(&pruned, "Gmagick"), "the subject argument: {source}");
    }
}

/// AN IMPORT IS A REFERENCE. This walk runs before name resolution, so nothing will later resolve
/// `ic()` back to `imagecreate` — by then the pruner has already decided.
#[test]
fn a_use_declaration_names_its_target() {
    let pruned = pruned_image(
        "<?php use function imagecolorallocate as ic;
         imagedestroy(imagecreatetruecolor(1, 1));
         ic($im, 1, 2, 3);",
    );
    assert!(kept(&pruned, "imagecolorallocate"));
}

/// A harvested literal has no syntax saying what it is, so it must be tried as a METHOD too —
/// otherwise `$obj->$m()` with `$m = 'clear'` loses every class declaring `clear`.
#[test]
fn harvesting_roots_methods_as_well_as_functions() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         $m = 'clear';
         $handler->$m();
         $f = $m; $f();",
    );
    assert!(
        kept(&pruned, "Imagick"),
        "a harvested literal that names a method must root the classes declaring it"
    );
}

/// `$obj->$m()` IS the dynamic dispatch, on its own, with no `$f()` anywhere to help.
///
/// The parser desugars it to `call_user_func([$obj, $m], …)`, so it arrives at the walk wearing an
/// array literal — the same shape a well-understood `['Imagick', 'clear']` wears. What separates
/// them is not the shape but whether anything inside it could be READ.
#[test]
fn a_dynamic_method_call_is_a_hazard_by_itself() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         $m = 'clear';
         $handler->$m();",
    );
    assert!(
        kept(&pruned, "Imagick"),
        "a dynamic method call must widen the roots with no other dynamic call present"
    );
}

/// The same, through the nullsafe spelling, which the parser keeps as its own node.
#[test]
fn a_nullsafe_dynamic_method_call_is_a_hazard_too() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         $m = 'clear';
         $handler?->$m();",
    );
    assert!(kept(&pruned, "Imagick"));
}

/// A first-class callable ROOTS WHAT CALLING IT WOULD ROOT.
///
/// `Imagick::queryFormats(...)` reaches the image prelude through the same detection as the
/// ordinary call, so the surface is injected either way — but the traversal used to record
/// nothing at all for the deferred form, and the pruner then removed the class the program is
/// about to call. The failure surfaced as `Undefined class: Imagick`, pointing at user code
/// rather than at the pass that deleted the declaration.
#[test]
fn a_static_first_class_callable_roots_its_class_and_method() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         $probe = Imagick::queryFormats(...);",
    );
    assert!(
        kept(&pruned, "Imagick"),
        "a static first-class callable must root the class it names"
    );
}

/// The instance spelling had the same hole, and it hid better: the arm scanned the RECEIVER, so
/// it looked handled. But the receiver's class is unknown before name resolution, which is
/// exactly why the ordinary method call roots on the METHOD NAME instead — and the deferred form
/// was dropping it.
#[test]
fn an_instance_first_class_callable_roots_its_method_name() {
    let pruned = pruned_image(
        "<?php imagedestroy(imagecreatetruecolor(1, 1));
         $probe = $handler->clear(...);",
    );
    assert!(
        kept(&pruned, "Imagick"),
        "an instance first-class callable must root every class declaring the method"
    );
}
