<?php
// Round-trip a small catalogue through ext/xmlwriter and ext/xml.

// -- writing: XMLWriter builds the document with indentation --

$writer = new XMLWriter();
$writer->openMemory();
$writer->setIndent(true);
$writer->startDocument('1.0', 'UTF-8');
$writer->startElement('catalogue');

$books = [
    ['isbn' => '978-0-13-468599-1', 'title' => 'The Pragmatic Programmer', 'price' => '42.50'],
    ['isbn' => '978-0-201-63361-0', 'title' => 'Design Patterns & Friends', 'price' => '54.99'],
];
foreach ($books as $book) {
    $writer->startElement('book');
    $writer->writeAttribute('isbn', $book['isbn']);
    $writer->writeElement('title', $book['title']);
    $writer->writeElement('price', $book['price']);
    $writer->endElement();
}

$writer->endElement();
$writer->endDocument();
$xml = $writer->outputMemory();
echo $xml;

// -- reading: an expat-style SAX parser with element and character handlers --

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
    echo "XML error ", $code, ": ", xml_error_string($code),
        " at line ", xml_get_current_line_number($parser), "\n";
    exit(1);
}

echo "titles: ", implode(' / ', $titles), "\n";

// -- xml_parse_into_struct: the whole document as a flat tag structure --

$struct = xml_parser_create();
xml_parser_set_option($struct, XML_OPTION_SKIP_WHITE, true);
xml_parse_into_struct($struct, $xml, $values, $index);
echo "tags: ", count($values), ", price entries at ", implode(',', $index['PRICE']), "\n";
