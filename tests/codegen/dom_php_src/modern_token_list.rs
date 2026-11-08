//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/token/list` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/token_list/add.phpt`: TokenList: add
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_add() {
    if skip_without_xml_native("test_add") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root/>');
$list = $dom->documentElement->classList;

$list->add();
$list->add('a', 'b');
$list->add('c');

$str = 'd';
$ref =& $str;

$list->add($ref);

echo $dom->saveXML(), "\n";

$list->value = '';
$list->add('e');

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root class="a b c d"/>
<?xml version="1.0" encoding="UTF-8"?>
<root class="e"/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/add_errors.phpt`: TokenList: add errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_add_errors() {
    if skip_without_xml_native("test_add_errors") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString("<root/>");
$list = $dom->documentElement->classList;

try {
    $list->add("");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->add("  ");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->add("\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->add(0);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"The empty string is not a valid token
The token must not contain any ASCII whitespace
Dom\TokenList::add(): Argument #1 must not contain any null bytes
Dom\TokenList::add(): Argument #1 must be of type string, int given
<?xml version="1.0" encoding="UTF-8"?>
<root/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/attlist.phpt`: TokenList: ATTLIST interaction
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_attlist() {
    if skip_without_xml_native("test_attlist") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8" ?>
<!DOCTYPE root [
    <!ELEMENT root EMPTY>
    <!ATTLIST child class CDATA "first second">
]>
<root><child/></root>
XML, LIBXML_DTDATTR);
$element = $dom->documentElement->firstChild;
$list = $element->classList;

echo 'class: ', $element->getAttribute('class'), "\n";
var_dump($list);

$list->remove('first');

var_dump($list);
echo $dom->saveXML(), "\n";

?>
"#);
    assert_exact(&out, r#"class: first second
object(Dom\TokenList)#2 (2) {
  ["length"]=>
  int(2)
  ["value"]=>
  string(12) "first second"
}
object(Dom\TokenList)#2 (2) {
  ["length"]=>
  int(1)
  ["value"]=>
  string(6) "second"
}
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
<!ELEMENT root EMPTY>
<!ATTLIST child class CDATA "first second">
]>
<root><child class="second"/></root>"#);
}

/// php-src `ext/dom/tests/modern/token_list/change_attribute.phpt`: TokenList: change attribute
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_change_attribute() {
    if skip_without_xml_native("test_change_attribute") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;
$list = $element->classList;

var_dump($list);

$element->attributes[0]->value = 'd';

var_dump($list);

$list->value = 'e f g';

var_dump($list);

"#);
    assert_exact(&out, r#"object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(3)
  ["value"]=>
  string(5) "a b c"
}
object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(1)
  ["value"]=>
  string(1) "d"
}
object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(3)
  ["value"]=>
  string(5) "e f g"
}"#);
}

/// php-src `ext/dom/tests/modern/token_list/clone.phpt`: TokenList: clone
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_clone() {
    if skip_without_xml_native("test_clone") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;
try {
  clone $element->classList;
} catch (Throwable $e) {
  echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#);
    assert_exact(&out, r#"Error: Trying to clone an uncloneable object of class Dom\TokenList"#);
}

/// php-src `ext/dom/tests/modern/token_list/contains.phpt`: TokenList: contains
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_contains() {
    if skip_without_xml_native("test_contains") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

echo "---\n";

var_dump($list->contains(''));
var_dump($list->contains('A'));
var_dump($list->contains('B'));
var_dump($list->contains('C'));

echo "---\n";

var_dump($list->contains(' A'));
var_dump($list->contains('B '));
var_dump($list->contains(' C '));

echo "---\n";

var_dump($list->contains('a'));
var_dump($list->contains('b'));
var_dump($list->contains('c'));

$element->setAttribute('class', 'D');

echo "---\n";

var_dump($list->contains('A'));
var_dump($list->contains('B'));
var_dump($list->contains('C'));
var_dump($list->contains('D'));

echo "---\n";

$list->value = 'E';
var_dump($list->contains('D'));
var_dump($list->contains('E'));

"#);
    assert_exact(&out, r#"---
bool(false)
bool(true)
bool(true)
bool(true)
---
bool(false)
bool(false)
bool(false)
---
bool(false)
bool(false)
bool(false)
---
bool(false)
bool(false)
bool(false)
bool(true)
---
bool(false)
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/token_list/contains_empty.phpt`: TokenList: contains empty
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_contains_empty() {
    if skip_without_xml_native("test_contains_empty") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root/>');
$element = $dom->documentElement;
$list = $element->classList;

var_dump($list->contains('x'));

"#);
    assert_exact(&out, r#"bool(false)"#);
}

/// php-src `ext/dom/tests/modern/token_list/contains_error.phpt`: TokenList: contains errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_contains_error() {
    if skip_without_xml_native("test_contains_error") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

try {
    $list->contains("\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Dom\TokenList::contains(): Argument #1 ($token) must not contain any null bytes"#);
}

/// php-src `ext/dom/tests/modern/token_list/count.phpt`: TokenList: count
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_count() {
    if skip_without_xml_native("test_count") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;
var_dump($element->classList->count(), count($element->classList), $element->classList->length);

"#);
    assert_exact(&out, r#"int(3)
int(3)
int(3)"#);
}

/// php-src `ext/dom/tests/modern/token_list/debug.phpt`: TokenList: debug
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_debug() {
    if skip_without_xml_native("test_debug") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;
var_dump($element->classList);

"#);
    assert_exact(&out, r#"object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(3)
  ["value"]=>
  string(5) "a b c"
}"#);
}

/// php-src `ext/dom/tests/modern/token_list/dimensions.phpt`: TokenList: dimensions
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dimensions() {
    if skip_without_xml_native("test_dimensions") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C 0"/>');
$list = $dom->documentElement->classList;

foreach (range(-1, 4) as $i) {
    echo "--- $i ---\n";
    var_dump($list[$i], isset($list[$i]), empty($list[$i]));
}

echo "--- \"0\" ---\n";
var_dump($list["0"], isset($list["0"]), empty($list["0"]));

echo "--- \"foo\" ---\n";
try {
    var_dump($list["foo"], isset($list["foo"]), empty($list["foo"]));
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

echo "--- 1.1 ---\n";
var_dump($list[1.1], isset($list[1.1]), empty($list[1.1]));

echo "--- true ---\n";
var_dump($list[true], isset($list[true]), empty($list[true]));

echo "--- false ---\n";
var_dump($list[false], isset($list[false]), empty($list[false]));

echo "--- ref ---\n";
$tmp = 2;
$ref =& $tmp;
var_dump($list[$ref], isset($list[$ref]), empty($list[$ref]));

"#);
    assert_format(&out, r#"--- -1 ---
NULL
bool(false)
bool(true)
--- 0 ---
string(1) "A"
bool(true)
bool(false)
--- 1 ---
string(1) "B"
bool(true)
bool(false)
--- 2 ---
string(1) "C"
bool(true)
bool(false)
--- 3 ---
string(1) "0"
bool(true)
bool(true)
--- 4 ---
NULL
bool(false)
bool(true)
--- "0" ---
string(1) "A"
bool(true)
bool(false)
--- "foo" ---
Cannot access offset of type string on Dom\TokenList
--- 1.1 ---

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d
string(1) "B"
bool(true)
bool(false)
--- true ---
string(1) "B"
bool(true)
bool(false)
--- false ---
string(1) "A"
bool(true)
bool(false)
--- ref ---
string(1) "C"
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/token_list/dimensions_error.phpt`: TokenList: dimensions error
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dimensions_error() {
    if skip_without_xml_native("test_dimensions_error") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$list = $dom->documentElement->classList;

$testOffsets = [
    new stdClass,
    [],
    fopen("php://output", "w"),
];

foreach ($testOffsets as $offset) {
    try {
        $list[$offset];
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }

    try {
        isset($list[$offset]);
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }

    try {
        empty($list[$offset]);
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}

try {
    $list[][0] = 1;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_format(&out, r#"Cannot access offset of type stdClass on Dom\TokenList
Cannot access offset of type stdClass in isset or empty
Cannot access offset of type stdClass in isset or empty
Cannot access offset of type array on Dom\TokenList
Cannot access offset of type array in isset or empty
Cannot access offset of type array in isset or empty

Warning: Resource ID#%d used as offset, casting to integer (%d) in %s on line %d

Warning: Resource ID#%d used as offset, casting to integer (%d) in %s on line %d

Warning: Resource ID#%d used as offset, casting to integer (%d) in %s on line %d
Cannot append to Dom\TokenList"#);
}

/// php-src `ext/dom/tests/modern/token_list/entities.phpt`: TokenList: entities interaction
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_entities() {
    if skip_without_xml_native("test_entities") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8" ?>
<!DOCTYPE root [
    <!ENTITY ent "foo">
]>
<root class="x&ent;x"/>
XML);
$element = $dom->documentElement;
$list = $element->classList;

var_dump($list);

var_dump($list->contains("xfoox"));
var_dump($list->contains("xx"));
var_dump($list->contains("foo"));

$list->add("test");

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(1)
  ["value"]=>
  string(5) "xfoox"
}
bool(true)
bool(false)
bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
<!ENTITY ent "foo">
]>
<root class="xfoox test"/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/equality.phpt`: TokenList: Test equality
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_equality() {
    if skip_without_xml_native("test_equality") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;

var_dump($element->classList === $element->classList);
$list = $element->classList;
var_dump($list === $list);
var_dump($list === $element->classList);

var_dump($list === $element->firstChild->classList);

"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/token_list/foreach_by_ref.phpt`: TokenList: foreach by ref
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_foreach_by_ref() {
    if skip_without_xml_native("test_foreach_by_ref") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');

try {
    foreach ($dom->documentElement->classList as &$class) {
    }
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"An iterator cannot be used with foreach by reference"#);
}

/// php-src `ext/dom/tests/modern/token_list/getIterator.phpt`: TokenList: getIterator
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_get_iterator() {
    if skip_without_xml_native("test_get_iterator") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

$it = $list->getIterator();
var_dump($it);

var_dump($it->key(), $it->current());
$it->next();
var_dump($it->key(), $it->current());
$it->next();
var_dump($it->key(), $it->current());
$it->next();
var_dump($it->key(), $it->current());

$it->rewind();
var_dump($it->key(), $it->current());

"#);
    assert_exact(&out, r#"object(InternalIterator)#5 (0) {
}
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
NULL
int(0)
string(1) "A""#);
}

/// php-src `ext/dom/tests/modern/token_list/gh18744.phpt`: GH-18744 (classList works not correctly if copy HTMLElement by clone keyword.)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh18744() {
    if skip_without_xml_native("test_gh18744") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$doc = \Dom\HTMLDocument::createEmpty();
$ele1 = $doc->createElement('div');
$ele1->classList->add('foo');
$ele2 = clone $ele1;
$ele2->classList->add('bar');

echo "Element1 class: " . $ele1->getAttribute('class');
echo "\n";
echo "Element2 class: " . $ele2->getAttribute('class');
echo "\n";

var_dump($ele1->classList !== $ele2->classList);
// These comparisons are not pointless: they're getters and should not create new objects
var_dump($ele1->classList === $ele1->classList);
var_dump($ele2->classList === $ele2->classList);

"#);
    assert_exact(&out, r#"Element1 class: foo
Element2 class: foo bar
bool(true)
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/token_list/item.phpt`: TokenList: item
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_item() {
    if skip_without_xml_native("test_item") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$list = $dom->documentElement->classList;

foreach (range(-1, 3) as $i) {
    var_dump($list->item($i));
}

echo "---\n";

$list->value = 'D';
var_dump($list->item(0));
var_dump($list->item(1));

"#);
    assert_exact(&out, r#"NULL
string(1) "A"
string(1) "B"
string(1) "C"
NULL
---
string(1) "D"
NULL"#);
}

/// php-src `ext/dom/tests/modern/token_list/iteration_01.phpt`: TokenList: iteration 01
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_iteration_01() {
    if skip_without_xml_native("test_iteration_01") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C D E F"/>');
$list = $dom->documentElement->classList;

foreach ($list as $i => $item) {
    var_dump($i, $item);
    $list->remove('A', 'D');
}

"#);
    assert_exact(&out, r#"int(0)
string(1) "A"
int(0)
string(1) "B"
int(1)
string(1) "C"
int(2)
string(1) "E"
int(3)
string(1) "F""#);
}

/// php-src `ext/dom/tests/modern/token_list/iteration_02.phpt`: TokenList: iteration 02
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_iteration_02() {
    if skip_without_xml_native("test_iteration_02") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C D E F"/>');
$list = $dom->documentElement->classList;

foreach ($list as $i => $item) {
    var_dump($i, $item);
    echo "==========\n";
    foreach ($list as $i2 => $item2) {
        var_dump($i2, $item2);
    }
    echo "==========\n";
}

"#);
    assert_exact(&out, r#"int(0)
string(1) "A"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
==========
int(1)
string(1) "B"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
==========
int(2)
string(1) "C"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
==========
int(3)
string(1) "D"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
==========
int(4)
string(1) "E"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
==========
int(5)
string(1) "F"
==========
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "C"
int(3)
string(1) "D"
int(4)
string(1) "E"
int(5)
string(1) "F"
=========="#);
}

/// php-src `ext/dom/tests/modern/token_list/iterator_modification_invalidation.phpt`: Test DOMTokenList iterator invalidation after modification
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_iterator_modification_invalidation() {
    if skip_without_xml_native("test_iterator_modification_invalidation") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C D"/>');
$list = $dom->documentElement->classList;


echo "---\n";

$counter = 0;
foreach ($list as $key => $token) {
    var_dump($key, $token);
    if (++$counter === 2) {
        $list->value = 'E F G';
    }
}

echo "---\n";

$iterator = $list->getIterator();
$iterator->next();
$list->value = 'X Y Z';
var_dump($iterator->key());
var_dump($iterator->current());
$iterator->rewind();
var_dump($iterator->key());
var_dump($iterator->current());
$list->value = '';
var_dump($iterator->key());
var_dump($iterator->current());
var_dump($iterator->valid());

"#);
    assert_exact(&out, r#"---
int(0)
string(1) "A"
int(1)
string(1) "B"
int(2)
string(1) "G"
---
int(1)
string(1) "Y"
int(0)
string(1) "X"
int(0)
NULL
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/token_list/remove.phpt`: TokenList: remove
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_remove() {
    if skip_without_xml_native("test_remove") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="test1    test2"/>');
$list = $dom->documentElement->classList;

$list->remove();
$list->remove('test1');

echo $dom->saveXML(), "\n";

$list->remove('nope');

echo $dom->saveXML(), "\n";

$list->remove('test2');

echo $dom->saveXML(), "\n";

$list->value = 'test3 test4';
$list->remove('test4');

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root class="test2"/>
<?xml version="1.0" encoding="UTF-8"?>
<root class="test2"/>
<?xml version="1.0" encoding="UTF-8"?>
<root class=""/>
<?xml version="1.0" encoding="UTF-8"?>
<root class="test3"/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/remove_errors.phpt`: TokenList: remove errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_remove_errors() {
    if skip_without_xml_native("test_remove_errors") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString("<root/>");
$list = $dom->documentElement->classList;

try {
    $list->remove("");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->remove("  ");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->remove("\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $list->remove(0);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"The empty string is not a valid token
The token must not contain any ASCII whitespace
Dom\TokenList::remove(): Argument #1 must not contain any null bytes
Dom\TokenList::remove(): Argument #1 must be of type string, int given
<?xml version="1.0" encoding="UTF-8"?>
<root/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/removed_element.phpt`: TokenList: operate on removed element
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_removed_element() {
    if skip_without_xml_native("test_removed_element") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

$element->remove();

var_dump($list);

$list->remove('B');

var_dump($list);

"#);
    assert_exact(&out, r#"object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(3)
  ["value"]=>
  string(5) "A B C"
}
object(Dom\TokenList)#3 (2) {
  ["length"]=>
  int(2)
  ["value"]=>
  string(3) "A C"
}"#);
}

/// php-src `ext/dom/tests/modern/token_list/replace.phpt`: TokenList: replace
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_replace() {
    if skip_without_xml_native("test_replace") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

var_dump($list->replace('nonexistent', 'X'));

echo $dom->saveXML(), "\n";

var_dump($list->replace('B', 'X'));

echo $dom->saveXML(), "\n";

var_dump($list->replace('C', 'X'));

echo $dom->saveXML(), "\n";

var_dump($list->replace('A', 'B'));

echo $dom->saveXML(), "\n";

var_dump($list->replace('X', 'B'));

echo $dom->saveXML(), "\n";

$list->value = 'A';
$list->replace('A', 'AA');

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<root class="A B C"/>
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="A X C"/>
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="A X"/>
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="B X"/>
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="B"/>
<?xml version="1.0" encoding="UTF-8"?>
<root class="AA"/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/replace_error.phpt`: TokenList: replace errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_replace_error() {
    if skip_without_xml_native("test_replace_error") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

try {
    $list->replace("\0", "X");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $list->replace("X", "\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $list->replace("a b", "X");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Dom\TokenList::replace(): Argument #1 ($token) must not contain any null bytes
Dom\TokenList::replace(): Argument #2 ($newToken) must not contain any null bytes
The token must not contain any ASCII whitespace"#);
}

/// php-src `ext/dom/tests/modern/token_list/supports.phpt`: TokenList: supports
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_supports() {
    if skip_without_xml_native("test_supports") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="a b c"><child/></root>');
$element = $dom->documentElement;
try {
    $element->classList->supports('a');
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Attribute "class" does not define any supported tokens"#);
}

/// php-src `ext/dom/tests/modern/token_list/toggle.phpt`: TokenList: toggle
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_toggle() {
    if skip_without_xml_native("test_toggle") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

echo "--- Toggle A (forced add) ---\n";

var_dump($list->toggle("A", true));

echo $dom->saveXML(), "\n";

echo "--- Toggle A (not forced) ---\n";

var_dump($list->toggle("A"));

echo $dom->saveXML(), "\n";

echo "--- Toggle A (forced remove) ---\n";

var_dump($list->toggle("A", false));

echo $dom->saveXML(), "\n";

echo "--- Toggle B (forced remove) ---\n";

var_dump($list->toggle("B", false));

echo $dom->saveXML(), "\n";

echo "--- Toggle D ---\n";

var_dump($list->toggle("D"));

echo $dom->saveXML(), "\n";

echo "--- Toggle C ---\n";

var_dump($list->toggle("C"));

echo $dom->saveXML(), "\n";

echo "--- Toggle E ---\n";

$list->value = 'E';
$list->toggle('E');

echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"--- Toggle A (forced add) ---
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="A B C"/>
--- Toggle A (not forced) ---
bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<root class="B C"/>
--- Toggle A (forced remove) ---
bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<root class="B C"/>
--- Toggle B (forced remove) ---
bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<root class="C"/>
--- Toggle D ---
bool(true)
<?xml version="1.0" encoding="UTF-8"?>
<root class="C D"/>
--- Toggle C ---
bool(false)
<?xml version="1.0" encoding="UTF-8"?>
<root class="D"/>
--- Toggle E ---
<?xml version="1.0" encoding="UTF-8"?>
<root class=""/>"#);
}

/// php-src `ext/dom/tests/modern/token_list/toggle_error.phpt`: TokenList: toggle errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_toggle_error() {
    if skip_without_xml_native("test_toggle_error") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root class="A B C"/>');
$element = $dom->documentElement;
$list = $element->classList;

try {
    $list->toggle("\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    $list->toggle("a b");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Dom\TokenList::toggle(): Argument #1 ($token) must not contain any null bytes
The token must not contain any ASCII whitespace"#);
}

/// php-src `ext/dom/tests/modern/token_list/value_edge_cases.phpt`: TokenList: value edge cases
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_value_edge_cases() {
    if skip_without_xml_native("test_value_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/token_list"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString('<root/>');
$list = $dom->documentElement->classList;

var_dump($list->value);

try {
    $list->value = "\0";
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

var_dump($list->value);

"#);
    assert_exact(&out, r#"string(0) ""
Value must not contain any null bytes
string(0) """#);
}
