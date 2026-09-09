<?php
// probe6c.php -- generated from the PROBE6C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe6c.php > fixtures/probe6c.out

/** `@line:col/byte` of the parser's current position. */
function position(XMLParser $parser): string
{
    return '@' . xml_get_current_line_number($parser) . ':' . xml_get_current_column_number($parser)
        . '/' . xml_get_current_byte_index($parser);
}

/** One xml_parse() of $doc with the listed handlers, then the summary line. */
function run(string $doc, string $handlers, bool $ns): void
{
    $p = $ns ? xml_parser_create_ns() : xml_parser_create();
    // 'end' rides along with 'start': xml_set_element_handler() sets both callbacks.
    $wanted = array_flip(explode(',', $handlers));
    if (isset($wanted['start'])) {
        xml_set_element_handler(
            $p,
            function ($parser, $name, $attrs) {
                echo '  S ', $name, ' ', json_encode($attrs), ' ', position($parser), "\n";
            },
            function ($parser, $name) {
                echo '  E ', $name, ' ', position($parser), "\n";
            }
        );
    }
    if (isset($wanted['cdata'])) {
        xml_set_character_data_handler($p, function ($parser, $data) {
            echo '  C ', var_export($data, true), ' ', position($parser), "\n";
        });
    }
    if (isset($wanted['default'])) {
        xml_set_default_handler($p, function ($parser, $data) {
            echo '  D ', var_export($data, true), ' ', position($parser), "\n";
        });
    }
    if (isset($wanted['pi'])) {
        xml_set_processing_instruction_handler($p, function ($parser, $target, $data) {
            echo '  PI ', var_export($target, true), ' ', var_export($data, true), ' ', position($parser), "\n";
        });
    }
    if (isset($wanted['ns'])) {
        xml_set_start_namespace_decl_handler($p, function ($parser, $prefix, $uri) {
            echo '  NS ', var_export($prefix, true), ' ', var_export($uri, true), ' ', position($parser), "\n";
        });
    }
    xml_set_notation_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId) {
        echo '  NOTATION ', json_encode([$name, $base, $systemId, $publicId]), ' ', position($parser), "\n";
    });
    xml_set_unparsed_entity_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId, $notation) {
        echo '  UNPARSED ', json_encode([$name, $base, $systemId, $publicId, $notation]), "\n";
    });
    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {
        echo '  EXTREF ', json_encode([$names, $base, $systemId, $publicId]), ' ', position($parser), "\n";
        return true;
    });
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo var_export($doc, true), ' [', $handlers, $ns ? ',NS' : '', ']', ' => ', $status, ' code=', $code, ' ', position($p), "\n";
}

echo "== xmlns attrs in non-ns mode\n";
run("<r xmlns='u' xmlns:p='v' p:a='1' xml:lang='en'/>", 'start', false);
run("<r xmlns='u' xmlns:p='v' p:a='1' xml:lang='en'/>", 'start,ns', true);
run("<r xmlns='u' xmlns:p='v' p:a='1' xml:lang='en'/>", 'default', false);
run("<r xmlns='u' xmlns:p='v' p:a='1' xml:lang='en'/>", 'default', true);
run("<p:r xmlns:p='v'><p:c/></p:r>", 'default', true);
run("<p:r xmlns:p='v'><p:c /></p:r>", 'default', true);
run("<r a='1' b = \"2\"\n  c='3'/>", 'default', false);
echo "== pi without data forms\n";
run("<r><?pi?><?pi ?><?pi  d  ?><?pi\nd?></r>", 'pi', false);
run("<r><?pi?><?pi ?><?pi  d  ?></r>", 'default', false);
run("<?xml-stylesheet href='a'?><r/>", 'pi', false);
echo "== entity value char refs\n";
run("<!DOCTYPE r [<!ENTITY e '&#38;#60;'><!ENTITY f '&#60;x'><!ENTITY g \"a'b\"><!ENTITY h '&amp;'>]><r>&e;|&f;|&g;|&h;</r>", 'cdata', false);
run("<!DOCTYPE r [<!ENTITY e 'v'>]><r a='&e;&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e '&#38;#60;'>]><r a='&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e '<'>]><r a='&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e 'a&f;b'><!ENTITY f 'F'>]><r a='&e;'/>", 'start', false);
run("<r a='&#65;&#x42;&#67;'/>", 'start', false);
run("<r a='x\ny\tz\r\nw'/>", 'start', false);
run("<r a='&#10;&#9;&#13;'/>", 'start', false);
run("<r a=\"'\" b='\"'/>", 'start', false);
echo "== positions of misc events\n";
run("<r>\n <!-- c -->\n <?p d?>\n <![CDATA[x]]>\n <![CDATA[]]>\n</r>", 'start,cdata,default,pi', false);
run("<!DOCTYPE r [<!NOTATION n SYSTEM 'y'><!ENTITY u SYSTEM 'z' NDATA n><!ENTITY x SYSTEM 'q'>]>\n<r>&x;</r>", 'start,cdata', false);
run("<r>a&amp;b&#65;c&lt;d</r>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v'>]><r>a&e;b</r>", 'start,cdata,default', false);
run("<r xmlns='u'><a xmlns:p='v'/></r>", 'start,ns', true);
echo "== error positions\n";
run("<r>\n  <a>\n</r>", 'start,cdata,pi', false);
run("<r>\n<a b='1'\n c='2' d>\n</r>", 'start,cdata,pi', false);
run("<r>&#xZZZ;</r>", 'start,cdata,pi', false);
run("<r>\n\n<!-- x -- y --></r>", 'start,cdata,pi', false);
run("<r>\n</r>junk", 'start,cdata,pi', false);
run("<r>\n</r>\n<x/>", 'start,cdata,pi', false);
run("<r>\n</r>\ntext", 'start,cdata,pi', false);
run("<r/>\n</r>", 'start,cdata,pi', false);
run("<r>\n<a>\n</b>\n</r>", 'start,cdata,pi', false);
run("<r>\n<a>\n</a", 'start,cdata,pi', false);
run("<r>\n<a>\n</a >\n</r>", 'start,cdata,pi', false);
run("<r>\n<a b='1' b='2'/></r>", 'start,cdata,pi', false);
run("<r>\n<a b=1></a></r>", 'start,cdata,pi', false);
run("<r>\n\n<a b='<'/></r>", 'start,cdata,pi', false);
run("<r>\n\n<a>&x;</a></r>", 'start,cdata,pi', false);
run("<r>\n\n<a>&x</a></r>", 'start,cdata,pi', false);
run("<r>\n\n<a>]]></a></r>", 'start,cdata,pi', false);
run("<r>\n<a>\n<![CDATA[x\n]]</a></r>", 'start,cdata,pi', false);
run("<r>\n<a>\n<?p x</a></r>", 'start,cdata,pi', false);
run("<r>\n<a>\n<!-- x</a></r>", 'start,cdata,pi', false);
run("<?xml version='1.0'?>\n\n<?xml version='1.0'?><r/>", 'start,cdata,pi', false);
run("\n\n  <?xml version='1.0'?><r/>", 'start,cdata,pi', false);
run("<r>\n<\xc3\xa9 b='1'>x</\xc3\xa9></r>", 'start,cdata,pi', false);
run("<r>\n  x\xff\n</r>", 'start,cdata,pi', false);
run("<r>\n  x\x00y\n</r>", 'start,cdata,pi', false);
run("<r>\n  <a b='x\x01'/>\n</r>", 'start,cdata,pi', false);
run("<r>\n<a>\n<b/>\n</a>\n<c/>\n</r>\n\n", 'start,cdata,pi', false);
run("<r>\r\n<a/>\r\n<b>\r\nx\r\n</b>\r\n</r>", 'start,cdata,pi', false);
run("<r>\r\n<a/>\r<b>\rx\r</b>\r</r>", 'start,cdata,pi', false);
run("<r a='1'\r\n b='2'\r\n/>", 'start,cdata,pi', false);
run("<!DOCTYPE r [\n<!ENTITY e 'v'>\n]>\n<r>&e;</r>", 'start,cdata,pi', false);
run("<!DOCTYPE r [\n<!ENTITY e 'v'>\n\n<r>&e;</r>", 'start,cdata,pi', false);
run("<!DOCTYPE r [\n<!ENTITY e 'v'>]\n<r>&e;</r>", 'start,cdata,pi', false);
run("<!DOCTYPE r\n  SYSTEM 'x.dtd'>\n<r/>", 'start,cdata,pi', false);
run("<!DOCTYPE>", 'start,cdata,pi', false);
run("<!DOCTYPE r", 'start,cdata,pi', false);
run("<!DOCTYPE r [<!ENTITY e 'v'>]", 'start,cdata,pi', false);
run("<!DOCTY", 'start,cdata,pi', false);
run("<!-- ", 'start,cdata,pi', false);
run("<?pi ", 'start,cdata,pi', false);
run("<![CDATA[", 'start,cdata,pi', false);
run("<r><![CDATA[", 'start,cdata,pi', false);
run("<r", 'start,cdata,pi', false);
run("<r ", 'start,cdata,pi', false);
run("<r a", 'start,cdata,pi', false);
run("<r a=", 'start,cdata,pi', false);
run("<r a='", 'start,cdata,pi', false);
run("<r a='1'", 'start,cdata,pi', false);
run("<r a='1' ", 'start,cdata,pi', false);
run("<r a='1'/", 'start,cdata,pi', false);
run("<r>", 'start,cdata,pi', false);
run("<r></", 'start,cdata,pi', false);
run("<r></r", 'start,cdata,pi', false);
run("<r>&", 'start,cdata,pi', false);
run("<r>&a", 'start,cdata,pi', false);
run("<r>&#", 'start,cdata,pi', false);
run("<r>&#1", 'start,cdata,pi', false);
run("<r>text", 'start,cdata,pi', false);
run("<", 'start,cdata,pi', false);
run("", 'start,cdata,pi', false);
run(" ", 'start,cdata,pi', false);
run("\n", 'start,cdata,pi', false);
run("x", 'start,cdata,pi', false);
run("<r/>x", 'start,cdata,pi', false);
run("<r/> ", 'start,cdata,pi', false);
run("<r/>\n\n", 'start,cdata,pi', false);
run("<r/><!--", 'start,cdata,pi', false);
run("<r/><?", 'start,cdata,pi', false);
run("<r/><!DOCTYPE r>", 'start,cdata,pi', false);
run("<!DOCTYPE r><!--c--><r/>", 'start,cdata,pi', false);
run("<!-- c --><!DOCTYPE r><r/>", 'start,cdata,pi', false);
run("<?p d?><!DOCTYPE r><r/>", 'start,cdata,pi', false);
run("<r/><!DOCTYPE r>", 'start,cdata,pi', false);
run("<r>&#x110000;</r>", 'start,cdata,pi', false);
run("<r>&#1114111;</r>", 'start,cdata,pi', false);
run("<r>&#xFFFF;</r>", 'start,cdata,pi', false);
run("<r>&#xFFFD;</r>", 'start,cdata,pi', false);
