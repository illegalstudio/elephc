<?php
// probe9c.php -- generated from the PROBE9C replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe9c.php > fixtures/probe9c.out

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
    echo var_export($doc, true), ' [', $handlers, $ns ? ',NS' : '', ']', ' => ', $status, ' code=', $code, ' ', position($p), "\n";
}

echo "== declared encodings\n";
run("<?xml version='1.0' encoding='us-ascii'?><a>x\xc3\xa9y</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='us-ascii'?><a>x\x85y</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='latin1'?><a>x\x85y\xff</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='ISO-8859-15'?><a>\xa4\xa6\xbe</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='windows-1252'?><a>\x80\x93</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='ISO-8859-2'?><a>\xb1</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding='bogus'?>   <a/>", 'start,cdata', false);
run("<?xml version='1.0' encoding='bogus'?>\n\n<a/>", 'start,cdata', false);
run("<?xml version='1.0' encoding='UTF-16'?>\n<a/>", 'start,cdata', false);
run("<?xml version='1.0' encoding='UTF-16LE'?><a/>", 'start,cdata', false);
run("<?xml version='1.0' encoding='UCS-4'?><a/>", 'start,cdata', false);
run("\xef\xbb\xbf<?xml version='1.0' encoding='latin1'?><a>\xc3\xa9</a>", 'start,cdata', false);
run("<?xml version='1.0' encoding = 'UTF-8' ?><a/>", 'start', false);
run("<?xml version='1.0' encoding='UTF-8'standalone='yes'?><a/>", 'start', false);
run("<?xml version='1.0' encoding='latin1'?><a b='\xe9'>\xe9<!--\xe9--><?p \xe9?><![CDATA[\xe9]]></a>", 'start,cdata,default', false);
run("<?xml version='1.0' encoding='latin1'?>\n<\xe9 \xe9='1'>\xe9\n\xe9</\xe9>", 'start,cdata', false);
run("<?xml version='1.0' encoding='latin1'?><a>\xe9</a>\xe9", 'start,cdata', false);
echo "== xmlns uri validity\n";
run("<r xmlns:p='a b'/>", 'start,ns', true);
run("<r xmlns:p='\xc3\xbc'/>", 'start,ns', true);
run("<r xmlns:p='http://x/y z'/>", 'start,ns', true);
run("<r xmlns:p='%zz'/>", 'start,ns', true);
run("<r xmlns:p='urn:x'/>", 'start,ns', true);
run("<r xmlns:p='#frag'/>", 'start,ns', true);
run("<r xmlns:p='a:b'/>", 'start,ns', true);
run("<r xmlns:p='::'/>", 'start,ns', true);
run("<r xmlns:p='http://[::1]/'/>", 'start,ns', true);
run("<r xmlns:p='a|b'/>", 'start,ns', true);
run("<r xmlns:p='a<b'/>", 'start,ns', true);
run("<r xmlns:p='a{b}'/>", 'start,ns', true);
run("<r xmlns:p='a^b'/>", 'start,ns', true);
run("<r xmlns:p='a`b'/>", 'start,ns', true);
run("<r xmlns:p='a\"b'/>", 'start,ns', true);
run("<r xmlns:p='a\\b'/>", 'start,ns', true);
run("<r xmlns:p='http://x/?q=1&r=2'/>", 'start,ns', true);
run("<r xmlns:p='a\tb'/>", 'start,ns', true);
run("<r xmlns:p='a\nb'/>", 'start,ns', true);
run("<r xmlns:p='%C3%BC'/>", 'start,ns', true);
run("<r xmlns:p='//x'/>", 'start,ns', true);
run("<r xmlns:p='?q'/>", 'start,ns', true);
run("<r xmlns:p='x#a#b'/>", 'start,ns', true);
run("<r xmlns='a b'/>", 'start,ns', true);
run("<r xmlns='\xc3\xbc'/>", 'start,ns', true);
echo "== pubid quotes\n";
run("<!DOCTYPE r PUBLIC 'p' 's'><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC \"p\" \"s\"><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC \"p\" 's'><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC 'p' \"s\"><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC 'p'>", 'start', false);
run("<!DOCTYPE r PUBLIC 'p q-()+,./:=?;!*#@\$_%' 's'><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC 'p\"' 's'><r/>", 'start', false);
run("<!DOCTYPE r PUBLIC 'p&' 's'><r/>", 'start', false);
run("<!DOCTYPE r SYSTEM 's&<>' [<!NOTATION n PUBLIC 'p' 's'>]><r/>", 'start', false);
run("<!DOCTYPE r [<!NOTATION n PUBLIC 'p'><!NOTATION m SYSTEM 's'><!NOTATION o PUBLIC 'p' 's'>]><r/>", 'start', false);
echo "== attr entity loops / nesting / undeclared in attr\n";
run("<!DOCTYPE r [<!ENTITY r '&r;'>]><r a='&r;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY a '&b;'><!ENTITY b '&a;'>]><r x='&a;'/>", 'start', false);
run("<r a='&nope;'/>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e 'x&nope;y'>]><r a='&e;'/>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e 'x&nope;y'>]><r>&e;</r>", 'start,default,cdata', false);
run("<!DOCTYPE r SYSTEM 'x'><r a='&nope;'/>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY x SYSTEM 'sys'>]><r a='&x;'/>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e '&#60;'>]><r a='&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e '&#38;#60;'>]><r>&e;</r>", 'start,cdata', false);
run("<!DOCTYPE r [<!ENTITY e 'a&lt;b'>]><r a='&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e 'a&#38;b'>]><r a='&e;'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e '\n'>]><r a='x&e;y'/>", 'start', false);
run("<!DOCTYPE r [<!ENTITY e 'v'><!ENTITY e 'w'>]><r a='&e;'/>", 'start', false);
run("<r a='&amp;&#38;&#x26;&lt;&gt;&quot;&apos;'/>", 'start', false);
run("<r a='&#x9;&#xA;&#xD;'/>", 'start', false);
run("<r a='&#x1;'/>", 'start', false);
run("<r a='&#xD800;'/>", 'start', false);
run("<r a='x&#xZ;'/>", 'start', false);
run("<r a='x&#'/>", 'start', false);
run("<r a='x&'/>", 'start', false);
run("<r a='x&e'/>", 'start', false);
run("<r a='x&e;'/>", 'start', false);
run("<r a='x&e;' b='2'/>", 'start', false);
echo "== standalone\n";
run("<?xml version='1.0' standalone='yes'?><!DOCTYPE r SYSTEM 'x'><r>&e;</r>", 'start,default', false);
run("<?xml version='1.0' standalone='no'?><!DOCTYPE r SYSTEM 'x'><r>&e;</r>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY % p 'x'>]><r>&e;</r>", 'start,default', false);
run("<!DOCTYPE r [%p;]><r>&e;</r>", 'start,default', false);
run("<!DOCTYPE r [<!ENTITY e 'a%p;b'>]><r>&e;</r>", 'start,default,cdata', false);
run("<!DOCTYPE r [<!ENTITY e 'a%p;b'>]><r>&f;</r>", 'start,default,cdata', false);
echo "== char refs in content: positions/line\n";
run("<r>&#10;x&#xA;\ny</r>", 'start,cdata', false);
run("<r>a&#38;#60;b</r>", 'start,cdata', false);
run("<r>&#x110000;</r>", 'start,cdata', false);
run("<r>&#xFFFFFFFFFF;</r>", 'start,cdata', false);
run("<r>&#99999999999999999999;</r>", 'start,cdata', false);
run("<r>&#x0000000000000041;</r>", 'start,cdata', false);
run("<r>&#00000000000000000065;</r>", 'start,cdata', false);
run("<r>&#xd7ff;&#xe000;&#xfffd;&#x10ffff;</r>", 'start,cdata', false);
run("<r>&#xd7ff;&#xd800;</r>", 'start,cdata', false);
echo "== misc\n";
run("<r><a/>\n\n<b/>\r\n\r\n<c/></r>", 'start,cdata', false);
run("<r>\t x \t</r>", 'start,cdata', false);
run("<r>x]]>y</r>", 'start,cdata', false);
run("<r>x]]y</r>", 'start,cdata', false);
run("<r>]]>", 'start,cdata', false);
run("<r>\xc3\xa9]]></r>", 'start,cdata', false);
run("<r>\xc3\xa9]]</r>", 'start,cdata', false);
run("<r>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\xc3\xa9bbbbbbbbbb</r>", 'start,cdata', false);
run("<r>\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9abcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc</r>", 'start,cdata', false);
run("<r>\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9&amp;x</r>", 'start,cdata', false);
run("<r>\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xac\xe2\x82\xacy</r>", 'start,cdata', false);
run("<r>\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80\xf0\x9f\x98\x80y</r>", 'start,cdata', false);
