//! Purpose:
//! Checks that erased SimpleXML dimension isset operations use presence rather than reads.
//!
//! Called from:
//! - The all-target AST-to-EIR regression suite.
//!
//! Key details:
//! - Reading a selected child would construct a wrapper with observable destruction.
//! - Runtime class guards retain the ordinary silent path for other mixed values.

use crate::ir::{Immediate, Op};

/// Every target keeps dimension presence in EIR even when the receiver has no object hint.
fn verify_erased_xml_presence(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function erasedDimensionIsset(mixed $node): bool { return isset($node['id']) && isset($node[0]); }
$xml = new SimpleXMLElement('<r id="1"/>'); var_dump(erasedDimensionIsset($xml));
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "erasedDimensionIsset").unwrap();
    let probes = function.instructions.iter().filter(|inst| inst.op == Op::MethodCall
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.strings[data.as_raw() as usize] == "__elephc_dimension_isset")).count();
    assert_eq!(probes, 2, "{target}: a presence probe must not materialize XML children");
    assert!(function.instructions.iter().any(|inst| inst.op == Op::InstanceOf),
        "{target}: non-XML values retain the ordinary silent read");
}

supported_target_cases!(erased_xml_presence, verify_erased_xml_presence);
