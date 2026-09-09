<?php
// probe8c.php -- generated from the PROBE8C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe8c.php > fixtures/probe8c.out

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
        echo '  UNPARSED ', json_encode([$name, $base, $systemId, $publicId, $notation]), ' ', position($parser), "\n";
    });
    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {
        echo '  EXTREF ', json_encode([$names, $base, $systemId, $publicId]), "\n";
        return true;
    });
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo var_export($doc, true), ' [', $handlers, $ns ? ',NS' : '', ']', ' => ', $status, ' code=', $code, ' ', position($p), "\n";
}

echo "== bom and cr\n";
run("\xef\xbb\xbf<r>x</r>", 'start,cdata', false);
run("\xef\xbb\xbf\n<r/>", 'start,cdata', false);
run("<r>a\rb</r>", 'start,cdata', false);
run("<r>a\r\rb\r</r>", 'start,cdata', false);
run("<r>\r<a/>\r</r>", 'start,cdata', false);
run("<r>\r\n<a/>\r\n</r>", 'start,cdata', false);
run("<r>\xc3\xa9\rb</r>", 'start,cdata', false);
run("<r>\xc3\xa9\r\nb</r>", 'start,cdata', false);
run("<r>\xc3\xa9\r\nb\r\n\xc3\xa9\r\n\r\nc</r>", 'start,cdata', false);
run("<r>a&amp;\rb</r>", 'start,cdata', false);
run("<r a='x\ry\r\nz\r'/>", 'start', false);
run("<r\r\n a='1'\r/>", 'start', false);
run("<r>\r</r>", 'start,cdata', false);
run("<r>\rx</r>", 'start,cdata', false);
run("<r>\r\r\rx</r>", 'start,cdata', false);
run("<r>\n\r\n\r\rx</r>", 'start,cdata', false);
run("<!-- a\rb\r\nc -->\r<r/>", 'start,default', false);
run("<r><![CDATA[a\rb\r\nc]]></r>", 'start,cdata', false);
run("<r><?p a\rb?></r>", 'start,pi', false);
run("<r>\r\n  x\r\n  y</r>", 'start,cdata', false);
run("<r>]</r>", 'start,cdata', false);
run("<r>]]</r>", 'start,cdata', false);
run("<r>a]]b</r>", 'start,cdata', false);
run("<r>a\x7fb</r>", 'start,cdata', false);
run("<r>\xc3\xa9</r>", 'start,cdata', false);
run("<r>a\xc3\xa9b</r>", 'start,cdata', false);
run("<r>\xc3\xa9\n\xc3\xa9</r>", 'start,cdata', false);
echo "== defaults / attlist normalization in ns and plain\n";
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd' b NMTOKENS #IMPLIED>]><r b='  x   y  '/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd' b NMTOKENS #IMPLIED>]><r b='  x   y  '/>", 'start', false);
run("<!DOCTYPE r [<!ATTLIST r xmlns:p CDATA 'urn:p' p:a CDATA 'v'>]><r p:b='1'/>", 'start,ns', true);
run("<!DOCTYPE r [<!ATTLIST r xmlns:p CDATA 'urn:p' p:a CDATA 'v'>]><r p:b='1'/>", 'start', false);
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd'>]><r a='given'/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a CDATA #FIXED 'd'>]><r/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a (x|y) 'x'>]><r a=' y '/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a ID #IMPLIED>]><r a=' y  z '/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a NMTOKEN #IMPLIED>]><r a='&#32;y&#32;'/>", 'start', true);
run("<!DOCTYPE r [<!ENTITY e ' v '><!ATTLIST r a NMTOKEN #IMPLIED>]><r a='&e;'/>", 'start', true);
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd'>]><r/>", 'default', true);
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd'>]><r/>", 'default', false);
echo "== ns edge\n";
run("<r xmlns:p='u' p:a='1' p:a='2'/>", 'start', true);
run("<r a='1' a='2'/>", 'start', true);
run("<r xmlns:xml='wrong'/>", 'start,ns', true);
run("<r xmlns:p='http://www.w3.org/XML/1998/namespace'/>", 'start,ns', true);
run("<r xmlns='http://www.w3.org/XML/1998/namespace'/>", 'start,ns', true);
run("<r xmlns:xmlns='u'/>", 'start,ns', true);
run("<r xmlns:p='http://www.w3.org/2000/xmlns/'/>", 'start,ns', true);
run("<r xmlns:p='not a uri'/>", 'start,ns', true);
run("<r xmlns='rel'/>", 'start,ns', true);
run("<r xmlns:p=''/>", 'start,ns', true);
run("<r xmlns=''/>", 'start,ns', true);
run("<p:r xmlns:p='u'><p:a xmlns:p=''/></p:r>", 'start,ns', true);
run("<r xmlns:p='u'><a xmlns:p=''><p:b/></a></r>", 'start,ns', true);
run("<r:a xmlns:r='u'></r:b>", 'start,ns', true);
run("<r:a xmlns:r='u'></a>", 'start,ns', true);
run("<r:a xmlns:r='u'></q:a>", 'start,ns', true);
run("<a></b>", 'start', true);
run("<p:a></p:a>", 'start', true);
run("<xml:a/>", 'start', true);
run("<r xml:a='1' xml:a='2'/>", 'start', true);
run("<r p:a='1' q:a='2'/>", 'start', true);
run("<r xmlns:p='u' xmlns:q='u' p:a='1' q:a='2' p:a='3'/>", 'start', true);
run("<r:x xmlns:r='u' r:x='1'/>", 'start,ns', true);
run("<r xmlns:p='u' xmlns:p='v'/>", 'start,ns', true);
run("<r xmlns='a' xmlns='b'/>", 'start,ns', true);
run("<r xmlns:p='u' p:a='1' xmlns:p='v'/>", 'start,ns', true);
run("<p:r xmlns:p='u'/>", 'default', true);
run("<p:r xmlns:p='u'>t</p:r>", 'default', true);
run("<r:a xmlns:r='u'></r:a >", 'start,ns', true);
run("<a:b:c xmlns:a='u'/>", 'start,ns', true);
run("<a: xmlns:a='u'/>", 'start,ns', true);
run("<:a/>", 'start', true);
run("<r :a='1'/>", 'start', true);
run("<r a:='1'/>", 'start', true);
echo "== end tag variants\n";
run("<r><a></a\n></r>", 'start', false);
run("<r><a></ a></r>", 'start', false);
run("<r><a></a x></r>", 'start', false);
run("<r><a></ab></r>", 'start', false);
run("<r><ab></a></r>", 'start', false);
run("<r><a></A></r>", 'start', false);
run("<r>\n<a>\n</\n</r>", 'start', false);
echo "== pi target / misc\n";
run("<r><?xml-stylesheet x?><?XML-x y?><?Xml z?></r>", 'start,pi', false);
run("<r><?a:b c?></r>", 'start,pi', false);
run("<?xml version='1.0'?>\n<r/>\n<?xml version='1.0'?>", 'start,pi', false);
run("<r><?xml version='1.0'?></r>", 'start,pi', false);
run("<r><!--->--></r>", 'start,default', false);
run("<r><!---></r>", 'start,default', false);
run("<r><!--a--->x</r>", 'start,default', false);
run("<r><!----></r>", 'start,default', false);
run("<r><!-- \xc3\xa9 -- x --></r>", 'start,default', false);
run("<r><!-- \xc3\xa9 --></r>", 'start,default', false);
run("<r><!--\x01--></r>", 'start,default', false);
run("<r><?p \x01?></r>", 'start,pi', false);
run("<r><![CDATA[\x01]]></r>", 'start,cdata', false);
run("<r><![CDATA[a]]]]></r>", 'start,cdata', false);
run("<r><![CDATA[a\xff]]></r>", 'start,cdata', false);
run("<r a='\xff'/>", 'start', false);
run("<r a='\xc3'/>", 'start', false);
run("<\xff/>", 'start', false);
run("<r\xff/>", 'start', false);
run("<r/>\xff", 'start', false);
run("<r>&\xc3\xa9;</r>", 'start,cdata', false);
run("<!DOCTYPE r [<!ENTITY \xc3\xa9 'v'>]><r>&\xc3\xa9;</r>", 'start,cdata,default', false);
