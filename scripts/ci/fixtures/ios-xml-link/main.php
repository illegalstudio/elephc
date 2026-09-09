<?php

// Both halves of the xml bridge: XMLWriter builds a document, then the expat-style
// SAX parser reads it back through element and character-data handlers.

$writer = new XMLWriter();
$writer->openMemory();
$writer->startDocument('1.0', 'UTF-8');
$writer->startElement('catalogue');
$writer->startElement('book');
$writer->writeAttribute('isbn', '978-0-13-468599-1');
$writer->writeElement('title', 'The Pragmatic Programmer');
$writer->endElement();
$writer->endElement();
$writer->endDocument();
$xml = $writer->outputMemory();

$parser = xml_parser_create();
xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);

$current = '';
$titles = [];
xml_set_element_handler(
    $parser,
    function (XMLParser $parser, string $name, array $attributes) use (&$current): void {
        $current = $name;
        if ($name === 'book') {
            echo "book ", $attributes['isbn'], "\n";
        }
    },
    function (XMLParser $parser, string $name) use (&$current): void {
        $current = '';
    },
);
xml_set_character_data_handler(
    $parser,
    function (XMLParser $parser, string $data) use (&$current, &$titles): void {
        if ($current === 'title') {
            $titles[] = $data;
        }
    },
);

if (xml_parse($parser, $xml, true) !== 1) {
    $code = xml_get_error_code($parser);
    echo "XML error ", $code, ": ", xml_error_string($code), "\n";
    exit(1);
}

echo "titles: ", implode(' / ', $titles), "\n";

#[Export]
function ios_xml_link_smoke(): int
{
    return 0;
}
