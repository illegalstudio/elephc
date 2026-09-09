<?php
// probe4c.php -- generated from the PROBE4C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe4c.php > fixtures/probe4c.out

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
                echo '  S ', $name, ' ', json_encode($attrs), "\n";
            },
            function ($parser, $name) {
                echo '  E ', $name, "\n";
            }
        );
    }
    if (isset($wanted['cdata'])) {
        xml_set_character_data_handler($p, function ($parser, $data) {
            echo '  C ', var_export($data, true), "\n";
        });
    }
    if (isset($wanted['default'])) {
        xml_set_default_handler($p, function ($parser, $data) {
            echo '  D ', var_export($data, true), "\n";
        });
    }
    if (isset($wanted['pi'])) {
        xml_set_processing_instruction_handler($p, function ($parser, $target, $data) {
            echo '  PI ', var_export($target, true), ' ', var_export($data, true), "\n";
        });
    }
    if (isset($wanted['ns'])) {
        xml_set_start_namespace_decl_handler($p, function ($parser, $prefix, $uri) {
            echo '  NS ', var_export($prefix, true), ' ', var_export($uri, true), "\n";
        });
    }
    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {
        echo '  EXTREF ', json_encode([$names, $base, $systemId, $publicId]), "\n";
        return true;
    });
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo var_export($doc, true), ' [', $handlers, ']', ' => ', $status, ' code=', $code, "\n";
}

echo "== entity expansion variants\n";
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'cdata', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'default', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'start,cdata', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\"><!ENTITY m \"<b>bb</b>\"><!ENTITY n \"&e;n\">]><r>a&e;b&m;c&n;d&amp;e&#65;</r>", '', false);
echo "== attr with entities\n";
run("<!DOCTYPE r [<!ENTITY e \"EE\">]><r a=\"x&e;y&amp;&#65;&lt;\"/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e \"EE\">]><r a=\"x&e;y&amp;&#65;&lt;\"/>", 'start,default', false);
run("<r a=\"  x  y\\t\\nz  \"/>", 'start', false);
echo "== external entity in content\n";
run("<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</r>", 'cdata', false);
run("<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</r>", 'cdata,default', false);
run("<!DOCTYPE r [<!ENTITY x PUBLIC \"pubid\" \"xsys\">]><r>a&x;b</r>", 'cdata', false);
run("<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r a=\"&x;\"/>", 'start', false);
echo "== param entity / external subset\n";
run("<!DOCTYPE r SYSTEM \"ext.dtd\"><r/>", 'start,default', false);
run("<!DOCTYPE r PUBLIC \"pub\" \"ext.dtd\" [<!ENTITY % pe \"x\">%pe;]><r/>", 'start,default', false);
echo "== standalone, decl edge\n";
run("<?xml version=\"1.0\" standalone=\"yes\"?><r/>", 'start,default', false);
run("<?xml version=\"1.1\"?><r/>", 'start', false);
run("<?xml version=\"2.0\"?><r/>", 'start', false);
run("<?xml encoding=\"UTF-8\"?><r/>", 'start', false);
run("<?xml version='1.0' ?>\n\n<r/>\n\n", 'start,default', false);
run(" <?xml version='1.0'?><r/>", 'start', false);
run("\xef\xbb\xbf<r/>", 'start', false);
run("<r/>\n<!-- c -->\n<?pi x?>\n", 'start,default', false);
echo "== whitespace in prolog / epilog\n";
run("\n\n<r/>", 'start,cdata,default', false);
echo "== comment/pi in DTD, default handler\n";
run("<!DOCTYPE r [<!-- dc --><?dpi x?><!ELEMENT r ANY>]><r/>", 'start,default', false);
echo "== cdata sections split\n";
run("<r><![CDATA[]]><![CDATA[a]]]]><![CDATA[>]]></r>", 'cdata', false);
run("<r><![CDATA[xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx]]></r>", 'cdata', false);
echo "== char refs\n";
run("<r>&#x20AC;&#8364;&#xD;&#10;&#x9;&#x10FFFF;&#xFFFE;&#xD800;&#x110000;</r>", 'cdata', false);
run("<r>&#x1;</r>", 'cdata', false);
run("<r>&#xFFFE;</r>", 'cdata', false);
run("<r>&#xD800;</r>", 'cdata', false);
run("<r>&#x110000;</r>", 'cdata', false);
run("<r>\x7f\x80</r>", 'cdata', false);
run("<r>\xef\xbf\xbe</r>", 'cdata', false);
run("<r>\xf4\x90\x80\x80</r>", 'cdata', false);
run("<r>\xed\xa0\x80</r>", 'cdata', false);
run("<r>\xc0\x80</r>", 'cdata', false);
run("<r>\x0c</r>", 'cdata', false);
run("<r a='\x01'/>", 'start', false);
run("<r>\r</r>", 'cdata', false);
run("<r>\r\n\r\n</r>", 'cdata', false);
run("<r>a\r</r>", 'cdata', false);
run("<r>a\rb</r>", 'cdata', false);
run("<r>a\r\nb\r\nc</r>", 'cdata', false);
run("<r>a\r\rb</r>", 'cdata', false);
run("<r>\xc3\xa9\r\n\xc3\xa9</r>", 'cdata', false);
run("<r>abc\xc3\xa9\r\ndef</r>", 'cdata', false);
echo "== names\n";
run("<r><a.b-c_d:e/><_x/><\xc3\xa9/><:a/><a:b:c/><-a/><.a/></r>", 'start', false);
run("<r><a:b:c/></r>", 'start', true);
run("<r xmlns:a='u'><a:b:c/></r>", 'start', true);
run("<r xmlns:xml='http://www.w3.org/XML/1998/namespace' xml:lang='en'/>", 'start', true);
run("<r xml:lang='en' xmlns:p='urn:p' p:x='1'/>", 'start', true);
run("<r xmlns='' xmlns:p=''/>", 'start', true);
run("<r xmlns:p='u'><p:a xmlns:p='u2'><p:b/></p:a><p:c/></r>", 'start', true);
echo "== attribute order and duplicates by case\n";
run("<r B='2' a='1' A='3'/>", 'start', false);
run("<r b='2' a='1'/>", 'start', false);
echo "== case folding of non-ascii\n";
run("<\xc3\xa9t\xc3\xa9 \xc3\xa0='1'>x</\xc3\xa9t\xc3\xa9>", 'start', false);
