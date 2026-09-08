//! Purpose:
//! Verifies Core class inventories exclude virtual properties and generated hook accessors.
//!
//! Called from:
//! - The codegen integration suite through `codegen::callables`.
//!
//! Key details:
//! - Direct, callable, inherited, trait, and native/eval metadata paths use the same expectations.
//! - A method whose name resembles a hook stays visible unless it really is an accessor.

use crate::support::*;

/// AOT, opaque eval, and native-by-name defaults populate backing storage without calling setters.
#[test]
fn test_core_hook_defaults_initialize_backing_storage_without_setter() {
    let declaration = r#"
class HookDefault {
    public int $value = 7 { get => $this->value; set { echo "set:"; $this->value = $value; } }
}
"#;
    let probes = r#"
echo get_class_vars("HookDefault")["value"], ":";
$object = new HookDefault();
echo $object->value, ":";
$object->value = 9;
echo $object->value;
"#;
    assert_eq!(compile_and_run(&format!("<?php {declaration} {probes}")), "7:7:set:9");
    for native in [false, true] {
        let (native_source, body) = if native {
            (declaration, probes.to_string())
        } else {
            ("", format!("{declaration} {probes}"))
        };
        let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
        let source = format!("<?php {native_source} $source = '{quoted}' . ' // ' . $argc; eval($source);");
        assert_eq!(compile_and_run(&source), "7:7:set:9", "native={native}");
    }
}

/// Returns a declaration whose virtual and backed hooks must not appear as ordinary methods.
fn hook_inventory_declaration() -> &'static str {
    r#"
class HookInventory {
    public int $ordinary = 1;
    public int $virtual { get => 42; }
    public int $closureOnly { get { $read = function() { return $this->closureOnly; }; return $read(); } }
    public int $backed { get => $this->backed; set { $this->backed = $value; } }
    public int $plain = 3;
    public function visible(): void {}
    public function __propget_plain(): void {}
}
"#
}

/// Returns shared probes for ordinary defaults and user methods through CUF and FCC.
fn hook_inventory_probes() -> &'static str {
    r#"
$vars = get_class_vars(HookInventory::class);
echo implode(',', array_keys($vars)), '|';
$varsCallback = get_class_vars(...);
echo count($varsCallback(HookInventory::class)), '|';
$methods = call_user_func('get_class_methods', HookInventory::class);
echo count($methods), ':', in_array('visible', $methods) ? 'V' : '-',
    ':', in_array('__propget_plain', $methods) ? 'U' : '-', '|';
$methodsCallback = get_class_methods(...);
echo count($methodsCallback(new HookInventory()));
echo '|', (new ReflectionProperty(HookInventory::class, 'virtual'))->isVirtual() ? 'V' : '-',
    (new ReflectionProperty(HookInventory::class, 'backed'))->isVirtual() ? '-' : 'B',
    (new ReflectionProperty(HookInventory::class, 'closureOnly'))->isVirtual() ? 'V' : '-';
"#
}

/// Native class inventories filter hooks while preserving uninitialized backed defaults as null.
#[test]
fn test_core_hook_introspection_aot_direct_and_callable() {
    let source = format!("<?php {} {}", hook_inventory_declaration(), hook_inventory_probes());
    assert_eq!(compile_and_run(&source), "ordinary,backed,plain|3|2:V:U|2|VBV");
}

/// Opaque eval class inventories match the AOT direct and callable projections.
#[test]
fn test_core_hook_introspection_eval_declarations() {
    let body = format!("{} {}", hook_inventory_declaration(), hook_inventory_probes());
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $source = '{quoted}' . ' // ' . $argc; eval($source);");
    assert_eq!(compile_and_run(&source), "ordinary,backed,plain|3|2:V:U|2|VBV");
}

/// Eval consumers of native metadata see virtual flags and can identify synthetic accessor rows.
#[test]
fn test_core_hook_introspection_eval_aot_metadata() {
    let quoted = hook_inventory_probes().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {} $source = '{quoted}' . ' // ' . $argc; eval($source);",
        hook_inventory_declaration());
    assert_eq!(compile_and_run(&source), "ordinary,backed,plain|3|2:V:U|2|VBV");
}

/// Trait hooks and inherited backing slots have the same visibility in native and eval inventories.
#[test]
fn test_core_hook_introspection_trait_and_inheritance() {
    let body = r#"
trait HookTrait {
    public int $virtual { get => 4; }
    public int $backed { get => $this->backed; }
    public function visible(): void {}
}

class HookParent {
    public int $existing = 2;
}
class HookChild extends HookParent {
    use HookTrait;
    public int $existing { get => 8; }
}
echo count(get_class_vars(HookTrait::class)), ':', count(get_class_methods(HookTrait::class)), '|';
$vars = get_class_vars(HookChild::class);
echo count($vars), ':', array_key_exists('existing', $vars) ? 'B' : '-',
    ':', array_key_exists('backed', $vars) ? 'B' : '-', '|';
echo count(get_class_methods(HookChild::class));
"#;
    assert_eq!(compile_and_run(&format!("<?php {body}")), "1:1|2:B:B|1");
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $source = '{quoted}' . ' // ' . $argc; eval($source);");
    assert_eq!(compile_and_run(&source), "1:1|2:B:B|1");
}

/// Shadowing a private hooked parent property cannot expose its inherited synthetic accessor.
#[test]
fn test_core_hook_introspection_private_parent_shadow() {
    let source = r#"<?php
class PrivateHookParent { private int $item { get => 9; } }
class PlainHookChild extends PrivateHookParent { public int $item = 4; }
echo count(get_class_methods(PlainHookChild::class)), ':', get_class_vars(PlainHookChild::class)['item'];
"#;
    assert_eq!(compile_and_run(source), "0:4");
}

/// Inherited eval accessors reach backing storage without reentering themselves or becoming readonly.
#[test]
fn test_core_hook_inherited_accessors_read_and_write_backing_storage() {
    let source = r#"<?php
$source = 'class InheritedHookParent {
    public int $value { get => $this->value; set { $this->value = $value + 1; } }
    public int $plainWrite { get => $this->plainWrite; }
}
class InheritedHookChild extends InheritedHookParent {
    public int $value = 2;
    public int $plainWrite = 5;
}
$child = new InheritedHookChild();
echo $child->value, ":";
$child->value = 3;
echo $child->value, "|", $child->plainWrite, ":";
$child->plainWrite = 6;
echo $child->plainWrite, "|", count(get_class_vars(InheritedHookChild::class));' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "2:4|5:6|2");
}

/// Parent-private accessor dispatch cannot select a child's same-named public hook.
#[test]
fn test_core_hook_private_parent_accessors_keep_their_owner() {
    let source = r#"<?php
$source = 'class PrivateHookOwner {
    private int $value { get => $this->value; set { $this->value = $value; } }
    public function write(int $value): void { $this->value = $value; }
    public function read(): int { return $this->value; }
}
class PublicHookShadow extends PrivateHookOwner {
    public int $value { get => 42; }
}
$child = new PublicHookShadow();
$child->write(7);
echo $child->read(), ":", $child->value;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "7:42");
}
