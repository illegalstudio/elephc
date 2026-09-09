<?php
// probe5c.php -- generated from the PROBE5C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe5c.php > fixtures/probe5c.out

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
    xml_set_notation_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId) {
        echo '  NOTATION ', json_encode([$name, $base, $systemId, $publicId]), "\n";
    });
    xml_set_unparsed_entity_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId, $notation) {
        echo '  UNPARSED ', json_encode([$name, $base, $systemId, $publicId, $notation]), "\n";
    });
    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {
        echo '  EXTREF ', json_encode([$names, $base, $systemId, $publicId]), "\n";
        return true;
    });
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo var_export($doc, true), ' => ', $status, ' code=', $code, ' ', position($p), "\n";
}

echo "== xml decl variants\n";
run("<?xml version='1.0'?><r/>", 'start,cdata,default', false);
run("<?xml version=\"1.0\" encoding='utf-8'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='UTF8'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='latin1'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='ISO-8859-15'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='us-ascii'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='UTF-16'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' standalone='maybe'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0'standalone='yes'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' standalone='yes' encoding='UTF-8'?><r/>", 'start,cdata,default', false);
run("<?xml version = '1.0' ?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0' foo='bar'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0'", 'start,cdata,default', false);
run("<?xml?><r/>", 'start,cdata,default', false);
run("<?XML version='1.0'?><r/>", 'start,cdata,default', false);
run("<?xml version='1.0'?>", 'start,cdata,default', false);
run("<?xml version='1.0'?><!DOCTYPE r><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r><!DOCTYPE r><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r (#PCDATA)><!ATTLIST r x CDATA 'def'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v'><!ENTITY e 'w'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY amp 'x'>]><r>&amp;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY lt '<'>]><r>&lt;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '&#38;'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '&#60;'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '&lt;'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '<x/>'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '<x>'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e '</r>'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a&f;b'><!ENTITY f 'F'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a&g;b'>]><r>&e;</r>", 'start,cdata,default', false);
run("<r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r SYSTEM 'x.dtd'><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY % p 'x'>]><r>%p;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY % p '<!ENTITY e \"v\">'>%p;]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e SYSTEM 'x'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e SYSTEM 'x'><!NOTATION n SYSTEM 'y'><!ENTITY u SYSTEM 'z' NDATA n>]><r>&u;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r ANY><!ATTLIST r a (x|y) 'x'>]><r a='z'/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ATTLIST r a CDATA #FIXED 'v'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ATTLIST r xmlns:p CDATA 'urn'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ATTLIST r a CDATA 'd'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r EMPTY>]><r>x</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [ ]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r[]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r []]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v'><![INCLUDE[<!ENTITY f 'w'>]]>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v' >]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'v' 'w'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!FOO>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r (a|b)*>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r (a,(b|c)+,d?)>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r (#PCDATA|a)*>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ELEMENT r (#PCDATA|a)>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ATTLIST r a ID #REQUIRED b IDREF #IMPLIED c NMTOKENS 'x y' d NOTATION (n) #IMPLIED>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ATTLIST r a FOO #IMPLIED>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!NOTATION n PUBLIC 'p'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!NOTATION n PUBLIC 'p' 's'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!NOTATION n 'p'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r PUBLIC 'p'><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r SYSTEM><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r SYSTEM 'a' 'b'><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r PUBLIC 'p\x01' 's'><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a<b'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a%p;b'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a&amp;b'>]><r>&e;</r>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a&#x;b'>]><r/>", 'start,cdata,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a&b'>]><r/>", 'start,cdata,default', false);
