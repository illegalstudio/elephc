//! Purpose:
//! Regressions for task 5f9dcd6d: packed writers and throwing capacity guards.
//!
//! Called from:
//! - The parent method-inlining EIR tests.
//!
//! Key details:
//! - Uses the executable sprite-batch example and checks every supported target.

use super::*;

/// Eight nested packed writes and a cold throw must lose their method-call overhead.
#[test]
fn sprite_batch_helpers_inline_on_every_target() {
    let source = include_str!("../../../examples/packed-sprite-batch/main.php");
    let throw_line = source.lines().position(|line| line.contains("throw new OverflowException")).unwrap() + 1;
    for target in TARGETS {
        let module = optimized(source, target);
        let add = method(&module, "SpriteBatch::add");
        let calls = called_methods(&module, add);
        assert!(!calls.iter().any(|name| matches!(name.as_str(), "vertex" | "reserve")),
            "{target}: sprite helpers survived: {calls:?}\n{}", crate::ir::print_function(add));
        let writes = placed(add).filter(|inst| inst.op == Op::PropSet && add.is_packed_field_access(inst)).count();
        assert_eq!(writes, 32, "{target}: all four vertices must be written in the caller");
        assert!(placed(add).any(|inst| inst.op == Op::ObjectNew
            && inst.span.is_some_and(|span| span.line as usize == throw_line)),
            "{target}: the cold exception must keep its construction line");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A user exception constructor and an extended builtin layout retain their real method frame.
#[test]
fn sprite_batch_guard_keeps_user_constructors_and_special_throwables_real() {
    let source = r#"<?php
class CustomOverflow extends RuntimeException {
    public function __construct() { parent::__construct('custom'); }
}
final class Guard {
    public function custom(bool $full): int {
        if ($full) { throw new CustomOverflow(); }
        return 1;
    }
    public function special(bool $full): int {
        if ($full) { throw new ErrorException('special', 0, 1); }
        return 2;
    }
}
function custom(Guard $guard, bool $full): int { return $guard->custom($full); }
function special(Guard $guard, bool $full): int { return $guard->special($full); }
echo custom(new Guard(), $argc > 3), special(new Guard(), $argc > 3);
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        for name in ["custom", "special"] {
            assert!(called_methods(&module, function(&module, name)).iter().any(|call| call == name),
                "{target}: {name} must retain its call boundary");
        }
    }
}

/// Ordinary property incdec retains scalar PHP locals with or without the XML capability loaded.
#[test]
fn ordinary_property_incdec_avoids_mixed_capture_locals_on_every_target() {
    let counter = r#"<?php
final class InlinePropertyCounter {
    private int $n = 4;
    private ?int $nullable = 0;
    private string $text = '4';
    private bool $flag = false;
    private float $number = 0.0;
    private ?float $optionalnumber = 0.0;
    private int|float $numeric = 0;
    public function boolpost(): bool { return $this->flag++; }
    public function boolpre(): bool { return ++$this->flag; }
    public function floatpost(): float { return $this->number++; }
    public function floatpre(): float { return ++$this->number; }
    public function optionalfloatpost(): ?float { return $this->optionalnumber++; }
    public function optionalfloatpre(): ?float { return ++$this->optionalnumber; }
    public function numericpost(): int|float { return $this->numeric++; }
    public function numericpre(): int|float { return ++$this->numeric; }
    public function nullablepost(): ?int { return $this->nullable++; }
    public function nullablepre(): ?int { return ++$this->nullable; }
    public function stringpost(): string { return $this->text++; }
    public function stringpre(): string { return ++$this->text; }
    public mixed $opaque = 0;
    public function mixedpost(): mixed { return $this->opaque++; }
    public function post(): int { return $this->n++; }
    public function pre(): int { return ++$this->n; }
    public function postdec(): int { return $this->n--; }
    public function predec(): int { return --$this->n; }
}
$counter = new InlinePropertyCounter();
echo $counter->post(), $counter->pre(), $counter->postdec(), $counter->predec();
"#;
    for xml in [false, true] {
        let source = format!("{counter}{}", if xml { "$xml = new SimpleXMLElement('<r/>');" } else { "" });
        for target in TARGETS {
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                &source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
            );
            for name in ["post", "pre", "postdec", "predec", "nullablepost", "nullablepre", "stringpost", "stringpre",
                "boolpost", "boolpre", "floatpost", "floatpre", "optionalfloatpost", "optionalfloatpre", "numericpost", "numericpre"] {
                let function = method(&module, &format!("InlinePropertyCounter::{name}"));
                assert!(!function.locals.iter().any(|local| local.name.as_deref()
                    .is_some_and(|name| name.contains("__elephc_incdec_new_"))),
                    "{target}, xml={xml}, {name}: the checked sum must feed the property directly");
                assert!(!function.locals.iter().any(|local| local.kind == crate::ir::LocalKind::PhpLocal
                    && local.php_type == crate::types::PhpType::Mixed
                    && local.name.as_deref().is_some_and(crate::names::is_generated_local_name)),
                    "{target}, xml={xml}, {name}: scalar property updates must not gain Mixed PHP locals");
            }
            let opaque = method(&module, "InlinePropertyCounter::mixedpost");
            assert!(opaque.locals.iter().any(|local| local.name.as_deref()
                .is_some_and(|name| name.contains("__elephc_incdec_new_"))),
                "{target}, xml={xml}: opaque/object arithmetic must retain its original owner capture");
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            crate::ir_passes::optimize_module(&mut module);
            let calls = called_methods(&module, function(&module, "main"));
            assert!(!calls.iter().any(|name| matches!(name.as_str(), "post" | "pre" | "postdec" | "predec")),
                "{target}, xml={xml}: ordinary scalar updates must remain inlinable: {calls:?}");
        }
    }
}
