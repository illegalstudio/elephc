<?php
// probe1c.php -- generated from the PROBE1C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe1c.php > fixtures/probe1c.out

/** `@line:col/byte` of the parser's current position. */
function position(XMLParser $parser): string
{
    return '@' . xml_get_current_line_number($parser) . ':' . xml_get_current_column_number($parser)
        . '/' . xml_get_current_byte_index($parser);
}

/** One xml_parse() of $doc with the listed handlers, then the RESULT line. */
function dump_events(string $doc, string $handlers, bool $ns, bool $caseFoldingOff): void
{
    $p = $ns ? xml_parser_create_ns() : xml_parser_create();
    if ($caseFoldingOff) {
        xml_parser_set_option($p, XML_OPTION_CASE_FOLDING, 0);
    }
    // 'end' rides along with 'start': xml_set_element_handler() sets both callbacks.
    $wanted = array_flip(explode(',', $handlers));
    if (isset($wanted['start'])) {
        xml_set_element_handler(
            $p,
            function ($parser, $name, $attrs) {
                echo 'START ', var_export($name, true), ' ', json_encode($attrs), ' ', position($parser), "\n";
            },
            function ($parser, $name) {
                echo 'END ', var_export($name, true), ' ', position($parser), "\n";
            }
        );
    }
    if (isset($wanted['cdata'])) {
        xml_set_character_data_handler($p, function ($parser, $data) {
            echo 'CDATA ', var_export($data, true), ' ', position($parser), "\n";
        });
    }
    if (isset($wanted['default'])) {
        xml_set_default_handler($p, function ($parser, $data) {
            echo 'DEFAULT ', var_export($data, true), "\n";
        });
    }
    if (isset($wanted['pi'])) {
        xml_set_processing_instruction_handler($p, function ($parser, $target, $data) {
            echo 'PI ', var_export($target, true), ' ', var_export($data, true), "\n";
        });
    }
    if (isset($wanted['ns'])) {
        xml_set_start_namespace_decl_handler($p, function ($parser, $prefix, $uri) {
            echo 'NSSTART ', var_export($prefix, true), ' ', var_export($uri, true), "\n";
        });
    }
    xml_set_notation_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId) {
        echo 'NOTATION ', json_encode([$name, $base, $systemId, $publicId]), "\n";
    });
    xml_set_unparsed_entity_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId, $notation) {
        echo 'UNPARSED ', json_encode([$name, $base, $systemId, $publicId, $notation]), "\n";
    });
    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {
        echo 'EXTREF ', json_encode([$names, $base, $systemId, $publicId]), "\n";
        return true;
    });
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo 'RESULT ', $status, ' code=', $code, ' msg=', var_export(xml_error_string($code), true), ' ', position($p), "\n-----\n";
}

echo "== basic\n";
dump_events("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- c -->\n<root a=\"1\" b='two'>\n  <item>hello &amp; world &lt;x&gt;</item>\n  <e/>\n  <?pi some data?>\n  <![CDATA[raw <cdata> & stuff]]>\n  text\n</root>\n", 'start,end,cdata,pi,default,ns', false, false);
echo "== no case folding, skip white\n";
dump_events("<Root>\n  <Item>  a  </Item>\n</Root>", 'start,end,cdata,pi,default,ns', false, true);
echo "== default only\n";
dump_events("<?xml version=\"1.0\"?>\n<!DOCTYPE root [<!ENTITY foo \"bar\">]>\n<root x='1'>t &foo; <!-- c --> <?pi d?> <![CDATA[cd]]></root>", 'default', false, false);
echo "== no handlers except default + start\n";
dump_events("<root x='1'>t<b/></root>", 'default,start', false, false);
echo "== ns\n";
dump_events("<r:root xmlns:r=\"urn:r\" xmlns=\"urn:d\" r:a=\"1\" b=\"2\"><child/><r:c/></r:root>", 'start,end,cdata,pi,default,ns', true, false);
echo "== error tag mismatch\n";
dump_events("<root><a></b></root>", 'start,end,cdata,pi,default,ns', false, false);
echo "== error syntax\n";
dump_events("<root><a>< </a></root>", 'start,end,cdata,pi,default,ns', false, false);
echo "== error junk after doc\n";
dump_events("<root/><x/>", 'start,end,cdata,pi,default,ns', false, false);
echo "== empty doc\n";
dump_events("", 'start,end,cdata,pi,default,ns', false, false);
echo "== unclosed\n";
dump_events("<root><a>", 'start,end,cdata,pi,default,ns', false, false);
echo "== entities\n";
dump_events("<!DOCTYPE r [<!ENTITY e \"EE\"><!NOTATION n SYSTEM \"nsys\"><!ENTITY u SYSTEM \"usys\" NDATA n><!ENTITY x SYSTEM \"xsys\">]><r>a&e;b&#65;&#x42;c&x;d</r>", 'start,end,cdata,pi,default,ns', false, false);
echo "== long text\n";
dump_events("<r>abcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghij</r>", 'cdata', false, false);
echo "== crlf\n";
dump_events("<r>line1\r\nline2\rline3</r>", 'cdata', false, false);
