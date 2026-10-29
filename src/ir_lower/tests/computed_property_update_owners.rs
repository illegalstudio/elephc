//! Purpose:
//! Verifies computed property updates preserve scalar and heap result storage.
//!
//! Called from:
//! - The AST-to-EIR regression suite.
//!
//! Key details:
//! - Ordinary getters and Stringable names coexist with the native XML protocol.

/// Computed updates validate and emit for scalar operands and concrete string getters.
#[test]
fn computed_property_update_storage_on_every_target() {
    let source = r#"<?php
class UpdateOwnerCounter {
    public int $number = 3;
    public string $text = 'a';
    public function receiver(): self { return $this; }
}
class UpdateOwnerName {
    public function __toString(): string { return 'number'; }
}
function updateOwnerExercise(UpdateOwnerCounter $counter): void {
    $counter->receiver()->number += 2;
    echo ++$counter->receiver()->number, $counter->receiver()->number++;
    echo ($counter->receiver()->text .= 'b');
    $counter->{new UpdateOwnerName()} += 1;
    echo $counter->{new UpdateOwnerName()};
}
updateOwnerExercise(new UpdateOwnerCounter());
function updateOwnerNumericMask(int|float $value): int { return $value & 7; }
echo updateOwnerNumericMask($argc);
$xml = new SimpleXMLElement('<r><a>1</a></r>');
$xml->a += 1;
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let numeric = module.functions.iter().find(|function| function.name == "updateOwnerNumericMask").unwrap();
        assert!(!numeric.instructions.iter().any(|inst| matches!(inst.op,
            crate::ir::Op::InstanceOf | crate::ir::Op::PushCallOperandOwner)),
            "{target}: scalar coercion must not gain native dispatch or operand owners");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
