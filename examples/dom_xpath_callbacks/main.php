<?php

$document = Dom\XMLDocument::createFromString(
    '<catalog><book>DOM</book><book>XPath</book></catalog>'
);
$xpath = new Dom\XPath($document);
$xpath->registerNamespace('app', 'https://example.com/xpath');
$xpath->registerPhpFunctionNS(
    'https://example.com/xpath',
    'inspect',
    function (array $nodes) use ($document) {
        var_dump($nodes[0] === $document->documentElement->firstChild);

        return 'XPath';
    },
);
echo $xpath->evaluate('app:inspect(//book)'), PHP_EOL;

$xpath->registerPhpFunctionNS(
    'https://example.com/xpath',
    'first',
    function (array $nodes) {
        return $nodes[0];
    },
);
$books = $xpath->evaluate('app:first(//book)');
echo get_class($books), ':', $books->length, ':';
echo $books->item(0)->textContent, PHP_EOL;

$xpath->registerNamespace('php', 'http://php.net/xpath');
$xpath->registerPhpFunctions([
    'summarize' => function (array $nodes) {
        return 'books:' . count($nodes);
    },
]);
echo $xpath->evaluate(
    "php:function('summarize', //book)",
), PHP_EOL;
