<?php
// Read, update, and query a small product catalog.
$catalog = new SimpleXMLElement('<catalog><product sku="A1"><name>Notebook</name><price>4.50</price></product></catalog>');
$product = $catalog->addChild('product');
$product->addAttribute('sku', 'B2');
$product->addChild('name', 'Pencil');
$product->addChild('price', '1.20');
foreach ($catalog->product as $product) {
    echo (string)$product['sku'], ': ', (string)$product->name, ' EUR ', (string)$product->price, "\n";
}
foreach ($catalog->xpath('//product[price < 2]') as $product) {
    echo 'Under EUR 2: ', (string)$product->name, "\n";
}
echo $catalog->asXML();

// The same factories accept PHP data URLs.
$inline = simplexml_load_file('data://text/plain;base64,PHIvPg==');
if ($inline instanceof SimpleXMLElement) { echo 'Inline document: ', $inline->getName(), "\n"; }
