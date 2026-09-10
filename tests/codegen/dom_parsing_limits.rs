//! Purpose:
//! Table-driven DOM parsing, encoding, parser-option, and post-failure-state regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_parsing_limits` through Rust's test harness.
//!
//! Key details:
//! - Each compact oracle case is deliberately independent so the costly bridge suite can run one filter at a time.
//! - Expectations pin PHP 8.5.8 with libxml2 2.15.3, including retained documents after parse failures.

use crate::support::compile_and_run;

/// Pins BOM, NUL, malformed UTF-8, recovery, tree-shaping, PARSEHUGE, and NO_XXE parser contracts.
#[test]
fn dom_parsing_limits_match_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-PARSE-ENCODING-01",
            r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
echo "bom|" . ($document->loadXML("\xEF\xBB\xBF<?xml version=\"1.0\"?><root/>") ? "T" : "F")
    . "|" . $document->documentElement->nodeName . "\n";

$document->loadXML("<before/>");
libxml_clear_errors();
$nul = $document->loadXML("<after/>\0");
$nulError = libxml_get_last_error();
echo "nul|" . ($nul ? "T" : "F") . "|" . $document->documentElement->nodeName
    . "|" . $nulError->code . "\n";

$invalid = new DOMDocument();
libxml_clear_errors();
$loaded = $invalid->loadXML("<root>\xC3\x28</root>");
$invalidError = libxml_get_last_error();
echo "utf8|" . ($loaded ? "T" : "F") . "|"
    . ($invalid->documentElement === null ? "N" : "X") . "|" . $invalidError->code;
"#,
            "bom|T|root\nnul|F|before|5\nutf8|F|N|81",
        ),
        (
            "DOM-PARSE-OPTIONS-02",
            r#"<?php
class NoXxeLoader {
    public mixed $context;
    public static int $calls = 0;

    public function __invoke($public, $system, $context): mixed {
        self::$calls++;
        return null;
    }
}

$loader = new NoXxeLoader();
libxml_set_external_entity_loader($loader);
libxml_use_internal_errors(true);
libxml_clear_errors();
$document = new DOMDocument();
$huge = $document->loadXML("<root/>", LIBXML_PARSEHUGE);
$blocked = $document->loadXML(
    "<!DOCTYPE root SYSTEM \"memory://blocked.dtd\"><root/>",
    LIBXML_DTDLOAD | LIBXML_NO_XXE,
);
echo "flags|" . ($huge ? "T" : "F") . "|" . ($blocked ? "T" : "F")
    . "|" . NoXxeLoader::$calls . "|" . count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
            "flags|T|T|0|0",
        ),
        (
            "DOM-PARSEHUGE-DEPTH-03",
            r#"<?php
$source = str_repeat("<node>", 257) . "payload" . str_repeat("</node>", 257);
libxml_use_internal_errors(true);
libxml_clear_errors();
try {
    Dom\XMLDocument::createFromString($source);
} catch (DOMException $error) {
    $parseError = libxml_get_last_error();
    echo "limit|" . get_class($error) . "|" . $error->getMessage() . "|"
        . $parseError->level . "/" . $parseError->code . "|"
        . $parseError->line . "/" . $parseError->column . "\n";
}

libxml_clear_errors();
$document = Dom\XMLDocument::createFromString($source, LIBXML_PARSEHUGE);
$node = $document->documentElement;
$depth = 1;
while ($node->firstElementChild !== null) {
    $node = $node->firstElementChild;
    $depth++;
}
echo "huge|" . $depth . "|" . $node->textContent . "|" . count(libxml_get_errors()) . "\n";

try {
    Dom\XMLDocument::createFromString("<node/>", LIBXML_PARSEHUGE | (1 << 30));
} catch (ValueError $error) {
    echo "invalid|" . $error->getMessage();
}
"#,
            "limit|DOMException|XML fragment is not well-formed|3/114|1/1542\nhuge|257|payload|0\ninvalid|Dom\\XMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)",
        ),
        (
            "DOM-PARSE-RECOVERY-03",
            r#"<?php
libxml_use_internal_errors(true);
$strict = new DOMDocument();
libxml_clear_errors();
$strictResult = $strict->loadXML("<root><child></root>");
$strictError = libxml_get_last_error();

$recover = new DOMDocument();
libxml_clear_errors();
$recoverResult = $recover->loadXML("<root><child></root>", LIBXML_RECOVER);
$recoverError = libxml_get_last_error();
echo "strict|" . ($strictResult ? "T" : "F") . "|"
    . ($strict->documentElement === null ? "N" : "X") . "|"
    . $strictError->level . "/" . $strictError->code . "\n";
echo "recover|" . ($recoverResult ? "T" : "F") . "|"
    . $recover->documentElement->nodeName . "|"
    . $recoverError->level . "/" . $recoverError->code;
"#,
            "strict|F|N|3/76\nrecover|T|root|3/76",
        ),
        (
            "DOM-PARSE-TREE-SHAPING-04",
            r#"<?php
$source = '<!DOCTYPE root [<!ENTITY e "expanded">]><root>' . "\x20\n <![CDATA[cdata]]>&e;</root>";
$plain = Dom\XMLDocument::createFromString($source);
$plainRoot = $plain->documentElement;
echo "plain|" . $plain->saveXml($plainRoot) . "|"
    . $plainRoot->childNodes->length . "|"
    . $plainRoot->firstChild->nodeType . "|"
    . $plainRoot->lastChild->nodeType . "\n";

$shaped = Dom\XMLDocument::createFromString(
    $source,
    LIBXML_NOENT | LIBXML_NOCDATA | LIBXML_NOBLANKS,
);
$shapedRoot = $shaped->documentElement;
echo "shaped|" . $shaped->saveXml($shapedRoot) . "|"
    . $shapedRoot->childNodes->length . "|"
    . $shapedRoot->firstChild->nodeType . "|"
    . $shapedRoot->lastChild->nodeType . "\n";

try {
    Dom\XMLDocument::createFromString('<root/>', 1 << 30);
} catch (ValueError $error) {
    echo "invalid|" . $error->getMessage();
}
"#,
            "plain|<root> \n <![CDATA[cdata]]>&e;</root>|3|3|5\nshaped|<root>cdataexpanded</root>|1|3|3\ninvalid|Dom\\XMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)",
        ),
        (
            "DOM-PARSE-NO-XXE-MODERN-05",
            r#"<?php
class NoXxeLoader {
    public mixed $context;
    public static int $calls = 0;

    public function __invoke($public, $system, $context): mixed {
        self::$calls++;
        return null;
    }
}

file_put_contents("dom-noxxe.dtd", '<!ELEMENT root EMPTY><!ATTLIST root external CDATA "yes">');
$source = '<!DOCTYPE root SYSTEM "dom-noxxe.dtd"><root/>';
libxml_use_internal_errors(true);
libxml_clear_errors();
libxml_set_external_entity_loader(new NoXxeLoader());
$string = Dom\XMLDocument::createFromString($source, LIBXML_DTDLOAD | LIBXML_DTDATTR | LIBXML_NO_XXE);
echo "string|" . ($string->documentElement->hasAttribute("external") ? "T" : "F") . "|" . NoXxeLoader::$calls . "|" . count(libxml_get_errors()) . "\n";
libxml_set_external_entity_loader(null);
file_put_contents("dom-noxxe.xml", $source);
libxml_clear_errors();
$file = Dom\XMLDocument::createFromFile("dom-noxxe.xml", LIBXML_DTDLOAD | LIBXML_DTDATTR | LIBXML_NO_XXE);
echo "file|" . ($file->documentElement->hasAttribute("external") ? "T" : "F") . "|" . count(libxml_get_errors()) . "\n";
libxml_clear_errors();
$inline = Dom\XMLDocument::createFromString('<!DOCTYPE root [<!ENTITY payload "inline">]><root>&payload;</root>', LIBXML_NOENT | LIBXML_NO_XXE);
echo "inline|" . $inline->documentElement->textContent . "|" . $inline->documentElement->firstChild->nodeType . "|" . count(libxml_get_errors());
unlink("dom-noxxe.dtd");
unlink("dom-noxxe.xml");
"#,
            "string|F|0|0\nfile|F|0\ninline|inline|3|0",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
