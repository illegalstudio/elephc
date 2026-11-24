//! Purpose:
//! PHP 8.4's `ReflectionConstant` global-constant reflector and its PHP 8.6 namespace helpers.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_constant_php86`.
//!
//! Key details:
//! - The constructor's constant name is resolved from the compiler's global constant table,
//!   so the value and name metadata are exact.
//! - php-src renders `Constant [ <type> <name> ] { <value> }`; the 8.6 rows are
//!   `getShortName()`, `getNamespaceName()` and `inNamespace()`.
//! - User constants keep their declaring file for `getFileName()`; internal constants answer
//!   `false` there and their module for `getExtensionName()`. A missing constant throws the
//!   same `ReflectionException` php-src raises.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// The global-constant reflector exposes the name parts, value, origin, deprecation state,
/// attributes and dump, and refuses a missing constant.
#[test]
fn test_reflection_constant_php86() {
    let out = compile_and_run_with_php_version(
        r#"<?php
const FOO = 1;
define('App\BAR', 2);
define('App\BAZ', 3);
$c = new ReflectionConstant('FOO');
echo $c->getName(), '|', $c->getShortName(), '|', $c->getNamespaceName(), '|', $c->inNamespace() ? 'in' : 'out', '|', $c->getValue(), '|', $c->isDeprecated() ? 'dep' : 'ok', '|', $c->getFileName() === __FILE__ ? 'thisfile' : $c->getFileName(), '|', $c->getExtensionName() === false ? 'noext' : $c->getExtensionName(), '|', $c->getExtension() === null ? 'nullext' : 'ext', '|', count($c->getAttributes()), "\n";
$d = new ReflectionConstant('App\BAR');
echo $d->getName(), '|', $d->getShortName(), '|', $d->getNamespaceName(), '|', $d->inNamespace() ? 'in' : 'out', '|', $d->getValue(), '|', $d->__toString();
$e = new ReflectionConstant('PHP_VERSION');
echo $e->getFileName() === false ? 'nofile' : 'file', '|', $e->getExtensionName(), '|', $e->getExtension() === null ? 'nullext' : 'ext', "\n";
$f = new ReflectionConstant('app\BAZ');
echo $f->getName(), "\n";
try { new ReflectionConstant('NOPE'); } catch (ReflectionException $error) { echo $error->getMessage(); }
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "FOO|FOO||out|1|ok|thisfile|noext|nullext|0\nApp\\BAR|BAR|App|in|2|Constant [ int App\\BAR ] { 2 }\nnofile|Core|nullext\nApp\\BAZ\nConstant \"NOPE\" does not exist"
    );
}
