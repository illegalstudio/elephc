//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `gh11830` folder, as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/dom_php_src/mod.rs`.
//!
//! Key details:
//! - Each test keeps the php-src program and expected output verbatim; its doc
//!   comment names the PHPT it was migrated from.
//! - A test Elephc does not pass yet is `#[ignore]`d with the missing behaviour, and
//!   php-src's own environment skips keep their `SKIPIF` message.

use super::*;

/// php-src `ext/dom/tests/gh11830/attribute_variation.phpt`: GH-11830 (ParentNode methods should perform their checks upfront) - attribute variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_attribute_variation() {
    if skip_without_xml_native("test_attribute_variation") {
        return;
    }
    let out = run_case(r#"gh11830"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container x="foo">
    <test/>
</container>
XML);

try {
    $doc->documentElement->firstElementChild->prepend($doc->documentElement->attributes[0]);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->append($doc->documentElement->attributes[0]);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->before($doc->documentElement->attributes[0]);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->after($doc->documentElement->attributes[0]);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->replaceWith($doc->documentElement->attributes[0]);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $doc->saveXML();
"#);
    assert_exact(&out, r#"Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
<?xml version="1.0"?>
<container x="foo">
    <test/>
</container>"#);
}

/// php-src `ext/dom/tests/gh11830/document_variation.phpt`: GH-11830 (ParentNode methods should perform their checks upfront) - document variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_document_variation() {
    if skip_without_xml_native("test_document_variation") {
        return;
    }
    let out = run_case(r#"gh11830"#, &[], r#"<?php
function test($method) {
    $otherDoc = new DOMDocument;
    $otherDoc->loadXML(<<<XML
    <?xml version="1.0"?>
    <other/>
    XML);

    $otherElement = $otherDoc->documentElement;

    $doc = new DOMDocument;
    $doc->loadXML(<<<XML
    <?xml version="1.0"?>
    <container>
        <alone/>
        <child><testelement/></child>
    </container>
    XML);

    $testElement = $doc->documentElement->firstElementChild->nextElementSibling->firstElementChild;

    try {
        $doc->documentElement->firstElementChild->$method($testElement, $otherElement);
    } catch (\DOMException $e) {
        echo $e->getMessage(), "\n";
    }

    echo $otherDoc->saveXML();
    echo $doc->saveXML();
}

test("prepend");
test("append");
test("before");
test("after");
test("replaceWith");
"#);
    assert_exact(&out, r#"Wrong Document Error
<?xml version="1.0"?>
<other/>
<?xml version="1.0"?>
<container>
    <alone/>
    <child/>
</container>
Wrong Document Error
<?xml version="1.0"?>
<other/>
<?xml version="1.0"?>
<container>
    <alone/>
    <child/>
</container>
Wrong Document Error
<?xml version="1.0"?>
<other/>
<?xml version="1.0"?>
<container>
    <alone/>
    <child/>
</container>
Wrong Document Error
<?xml version="1.0"?>
<other/>
<?xml version="1.0"?>
<container>
    <alone/>
    <child/>
</container>
Wrong Document Error
<?xml version="1.0"?>
<other/>
<?xml version="1.0"?>
<container>
    <alone/>
    <child/>
</container>"#);
}

/// php-src `ext/dom/tests/gh11830/hierarchy_variation.phpt`: GH-11830 (ParentNode methods should perform their checks upfront) - hierarchy variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_hierarchy_variation() {
    if skip_without_xml_native("test_hierarchy_variation") {
        return;
    }
    let out = run_case(r#"gh11830"#, &[], r#"<?php
function test($method) {
    $doc = new DOMDocument;
    $doc->loadXML(<<<XML
    <?xml version="1.0"?>
    <container>
        <alone/>
        <child><testelement/></child>
    </container>
    XML);

    $container = $doc->documentElement;
    $alone = $container->firstElementChild;
    $testElement = $alone->nextElementSibling->firstElementChild;

    try {
        $testElement->$method($alone, $container);
    } catch (\DOMException $e) {
        echo $e->getMessage(), "\n";
    }

    echo $doc->saveXML();
}

test("prepend");
test("append");
test("before");
test("after");
test("replaceWith");
"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0"?>
Hierarchy Request Error
<?xml version="1.0"?>
Hierarchy Request Error
<?xml version="1.0"?>
Hierarchy Request Error
<?xml version="1.0"?>
Hierarchy Request Error
<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/gh11830/type_variation.phpt`: GH-11830 (ParentNode methods should perform their checks upfront) - type variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_type_variation() {
    if skip_without_xml_native("test_type_variation") {
        return;
    }
    let out = run_case(r#"gh11830"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <test/>
    <child><testelement/></child>
</container>
XML);

$testElement = $doc->documentElement->firstElementChild->nextElementSibling->firstElementChild;

try {
    $doc->documentElement->firstElementChild->prepend($testElement, 0);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->append($testElement, true);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->before($testElement, null);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->after($testElement, new stdClass);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $doc->documentElement->firstElementChild->replaceWith($testElement, []);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}

echo $doc->saveXML();
"#);
    assert_exact(&out, r#"DOMElement::prepend(): Argument #2 must be of type DOMNode|string, int given
DOMElement::append(): Argument #2 must be of type DOMNode|string, bool given
DOMElement::before(): Argument #2 must be of type DOMNode|string, null given
DOMElement::after(): Argument #2 must be of type DOMNode|string, stdClass given
DOMElement::replaceWith(): Argument #2 must be of type DOMNode|string, array given
<?xml version="1.0"?>
<container>
    <test/>
    <child><testelement/></child>
</container>"#);
}
