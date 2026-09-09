<?php
// probe2misc.php -- generated from the PROBE2MISC replay table in
// crates/elephc-xml/src/parser/tests/corpus_data.rs by
//   cargo test -p elephc-xml --lib -- regen --ignored
// Do not edit by hand. Re-records its fixture under PHP 8.5.10 / libxml2 2.15.3:
//   php fixtures/probe2misc.php > fixtures/probe2misc.out

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
    $status = xml_parse($p, $doc, true);
    $code = xml_get_error_code($p);
    echo var_export($doc, true), ' => ', $status, ' code=', $code, " '", xml_error_string($code), "' ", position($p), "\n";
}

echo "== misc errors\n";
run("<a b>x</a>", 'cdata', false);
run("<a b=1/>", 'cdata', false);
run("<a b='1' b='2'/>", 'cdata', false);
run("<a b='<'/>", 'cdata', false);
run("<a>&foo</a>", 'cdata', false);
run("<a>]]></a>", 'cdata', false);
run("<!-- -- --><a/>", 'cdata', false);
run("<?xml version='1.0'?><?xml version='1.0'?><a/>", 'cdata', false);
run("<a><![CDATA[x</a>", 'cdata', false);
run("<a>&#xZZ;</a>", 'cdata', false);
run("<a>&#0;</a>", 'cdata', false);
run("<a b='1'c='2'/>", 'cdata', false);
run("<a b 1/>", 'cdata', false);
run("<a/><!-- c", 'cdata', false);
run("<a", 'cdata', false);
run("<a b='1'", 'cdata', false);
run("<a></a", 'cdata', false);
run("<?xml version='1.0' encoding='bogus'?><a/>", 'cdata', false);
run("<?xml version='1.0' encoding='ISO-8859-1'?><a>\xe9</a>", 'cdata', false);
run("<a>\xe9</a>", 'cdata', false);
run("<a>\x01</a>", 'cdata', false);
run("<a><b></c></a>", 'cdata', false);
run("<a><!DOCTYPE x><b/></a>", 'cdata', false);
run("<!DOCTYPE a [<!ENTITY r '&r;'>]><a>&r;</a>", 'cdata', false);
run("<a xmlns:p='u'><p:b/></a>", 'cdata', false);
run("<a><p:b/></a>", 'cdata', false);
run("<1a/>", 'cdata', false);
run("<a>&lt</a>", 'cdata', false);
run("<a>&#;</a>", 'cdata', false);
run("<a>&;</a>", 'cdata', false);
