use super::*;

#[test]
fn test_constructor_promoted_properties() {
    let out = compile_and_run(
        r#"<?php
class User {
    public function __construct(public int $id, private string $name = "Ada") {}
    public function name() { return $this->name; }
}
$u = new User(7);
echo $u->id;
echo ":";
echo $u->name();
"#,
    );
    assert_eq!(out, "7:Ada");
}

#[test]
fn test_constructor_promoted_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class Token {
    public function __construct(public readonly int $id) {}
    public function id() { return $this->id; }
}
$token = new Token(42);
echo $token->id();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_constructor_promoted_by_ref_property_reads_source_updates() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$value = 2;
echo $box->value;
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_constructor_promoted_by_ref_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$box->value = 3;
echo $value;
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_constructor_promoted_by_ref_string_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public string &$name) {}
}
$name = "Ada";
$box = new Box($name);
$box->name = "Grace";
echo $name;
"#,
    );
    assert_eq!(out, "Grace");
}
