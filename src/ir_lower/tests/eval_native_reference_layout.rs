//! Purpose:
//! Preserves native reference payload layouts when dynamic eval invalidates local type facts.
//!
//! Called from:
//! - AST-to-EIR regression tests in CI for every supported target.
//!
//! Key details:
//! - A shared string/int/float property cell cannot be widened to Mixed behind its aliases.

/// An opaque eval barrier preserves string and scalar cells shared with native properties.
#[test]
fn dynamic_eval_keeps_native_reference_payload_layouts_on_every_target() {
    let source = r#"<?php
class EvalReferenceLayout {
    public string $text = 'before';
    public int $number = 1;
    public float $weight = 1.5;
    public ?int $maybe = null;
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
    public function &weightRef(): float { return $this->weight; }
    public function &maybeRef(): ?int { return $this->maybe; }
}
$box = new EvalReferenceLayout();
$text =& $box->textRef();
$number =& $box->numberRef();
$weight =& $box->weightRef();
$maybe =& $box->maybeRef();
$source = $argc > 0 ? '$box->text = "after"; $number = 2; $weight = 2.5; $maybe = 3;' : '';
eval($source);
echo $text, $number, $weight, $maybe;
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        for (name, ty) in [("text", crate::types::PhpType::Str),
            ("number", crate::types::PhpType::Int), ("weight", crate::types::PhpType::Float),
            ("maybe", crate::types::PhpType::Union(vec![crate::types::PhpType::Int, crate::types::PhpType::Void]).codegen_repr())] {
            let local = main.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), ty, "{target}: {name} lost its shared cell layout");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
