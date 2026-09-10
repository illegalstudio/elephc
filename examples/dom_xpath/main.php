<?php

$document = Dom\XMLDocument::createFromString(
    '<root xmlns:p="urn:example">'
    . '<p:item>first</p:item><p:item>second</p:item>'
    . '</root>'
);
$xpath = new Dom\XPath($document);

$items = $xpath->query('//p:item');
echo $items->length, " items: ";
echo $items->item(0)->textContent, ', ';
echo $items->item(1)->textContent, "\n";
echo 'count = ', $xpath->evaluate('count(//p:item)'), "\n";

$xpath->registerNodeNamespaces = false;
$xpath->registerNamespace('p', 'urn:example');
echo 'persistent namespace count = ';
echo $xpath->query('//p:item')->length, "\n";
