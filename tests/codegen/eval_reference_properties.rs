//! Purpose:
//! Regressions for properties bound by reference (`$this->a = &$this->b`) when the program also
//! links the eval bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_reference_propert` through Rust's test harness.
//!
//! Key details:
//! - A reference property keeps a managed cell pointer where a plain slot keeps its value. The
//!   bridge's generated property helpers (`__elephc_eval_value_property_get/set/is_initialized`)
//!   read the slot as if it held the value, so a read through a `mixed` receiver in a frame with
//!   an eval context answered the cell pointer's bytes: empty strings and null. The classic DOM
//!   prelude binds `data`, `nodeValue` and `textContent` this way, which emptied every attribute
//!   and text value of a DOM tree in any program that also used eval (Termwind under Laravel).
//! - Expected output is real php 8.5 output.

use crate::support::compile_and_run;

/// Verifies reads and writes of aliased properties agree between compiled code and eval code.
#[test]
fn test_eval_reads_and_writes_reference_bound_properties() {
    let out = compile_and_run(
        r#"<?php
class Nd {
    public ?string $nodeValue = null;
    public ?string $textContent = '';
    public ?string $value = null;
    public string $name = '';
}
class At extends Nd {
    public function __construct(string $name, mixed $value = '') {
        $this->name = $name;
        $this->value = (string) $value;
        $this->nodeValue = &$this->value;
        $this->textContent = &$this->value;
    }
}
class El extends Nd {
    public array $attrs = [];
    public function set(string $k, mixed $v): At {
        $a = new At($k, $v);
        $this->attrs[$k] = $a;
        return $a;
    }
    public function get(string $k): string {
        if (isset($this->attrs[$k])) {
            return $this->attrs[$k]->value;
        }
        return '';
    }
}
$e = new El;
$e->set('class', 'k');
echo "compiled read: ", $e->get('class'), "\n";
$a = $e->attrs['class'];
echo "eval read: ", eval('return $a->value . "|" . $a->nodeValue . "|" . $a->textContent;'), "\n";
eval('$a->nodeValue = "from-eval";');
echo "after eval write: ", $a->value, "|", $e->get('class'), "|", $a->textContent, "\n";
$a->value = 'compiled-again';
echo "eval sees: ", eval('return $a->nodeValue;'), "\n";
"#,
    );
    assert_eq!(
        out,
        "compiled read: k\neval read: k|k|k\nafter eval write: from-eval|from-eval|from-eval\neval sees: compiled-again\n"
    );
}

/// Verifies a DOM tree keeps its attribute and text values in a program that links eval.
#[test]
fn test_eval_linked_program_reads_dom_attribute_and_text_values() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument;
$dom->loadHTML('<?xml encoding="UTF-8"><!DOCTYPE html><html><body><div class="flex mx-2"><span class="px-1">info</span></div></body></html>', LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL);
$body = $dom->getElementsByTagName('body')->item(0);
$span = $body->firstChild->firstChild;
$txt = $span->firstChild;
echo "A:", $span->getAttribute('class'), "\n";
echo "B:", $txt->nodeValue, "\n";
foreach ($span->attributes as $a) { echo "C:", $a->name, '=', $a->value, "\n"; }
echo "D:", $txt->data, "\n";
if ($argc > 5) { eval("1;"); }
"#,
    );
    assert_eq!(out, "A:px-1\nB:info\nC:class=px-1\nD:info\n");
}
