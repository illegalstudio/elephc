//! Purpose:
//! Resolution of run-time callable arrays `[$object, $method]` through the composite lookup
//! tables (`__rt_callable_lookup_composite_*`) when the call site's class declares private methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests lookup_table_sharing` through Rust's test harness.
//!
//! Key details:
//! - Written inside a class `S`, `[$child, 'm']` resolves to `S`'s private `m` even when the
//!   child declares a public `m` of its own: php checks the callable from `S`'s scope. A site with
//!   more candidates than the inline ladder answers that from its scoped rows before the public
//!   table, which must therefore not answer the child's method first.

use crate::support::*;

/// Two classes with private methods call a run-time `[$object, $method]`; each has a descendant
/// redeclaring that private method's name as public. The scope's private method wins, as in php.
#[test]
fn test_scoped_callable_array_sites_prefer_the_scope_private_method() {
    let mut source = String::from("<?php\n");
    for class in ["Alpha", "Beta", "Gamma"] {
        source.push_str(&format!("class {class} {{\n"));
        for index in 0..12 {
            source.push_str(&format!(
                "    public function m{index}(string $v): string {{ return '{class}{index}:' . $v; }}\n"
            ));
        }
        source.push_str("}\n");
    }
    for (class, child, private) in [("SiteOne", "SubOne", "m3"), ("SiteTwo", "SubTwo", "m5")] {
        source.push_str(&format!(
            "class {class} {{\n    private function {private}(string $v): string {{ return '{class}-private:' . $v; }}\n    public function run(object $o, string $m): string {{ $c = [$o, $m]; return $c('x'); }}\n}}\nclass {child} extends {class} {{\n    public function {private}(string $v): string {{ return '{child}-public:' . $v; }}\n}}\n"
        ));
    }
    source.push_str(
        "$m = $argc > 5 ? 'm1' : 'm3';\necho (new SiteOne())->run(new SubOne(), $m), ' ', (new SiteTwo())->run(new SubTwo(), 'm5'), ' ', (new SiteOne())->run(new Beta(), 'm11'), \"\\n\";\n",
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "SiteOne-private:x SiteTwo-private:x Beta11:x\n");
}
