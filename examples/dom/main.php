<?php
// The classic DOM extension: build a document, serialize it, parse XML and HTML back, and walk the tree.

// 1. Build and serialize an XML document (the shape of Symfony's XmlDescriptor).
$dom = new DOMDocument('1.0', 'UTF-8');
$dom->appendChild($routes = $dom->createElement('routes'));
foreach (['home' => '/', 'blog' => '/blog/{slug}'] as $name => $path) {
    $routes->appendChild($route = $dom->createElement('route'));
    $route->setAttribute('name', $name);
    $route->appendChild(new DOMText($path));
}
$dom->formatOutput = true;
echo $dom->saveXML();

// 2. Parse it back and read it.
$parsed = new DOMDocument();
$parsed->loadXML($dom->saveXML());
foreach ($parsed->getElementsByTagName('route') as $route) {
    echo $route->getAttribute('name'), ' => ', $route->textContent, "\n";
}

// 3. Parse HTML the way Termwind does and walk the tree.
$html = new DOMDocument();
$html->loadHTML(
    '<?xml encoding="UTF-8"><div class="box"><b>Hello</b> <i>wor&amp;ld</i></div>',
    LIBXML_NOERROR | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS
);
$body = $html->getElementsByTagName('body')->item(0);
foreach ($body->firstChild->childNodes as $node) {
    echo $node->nodeName, ': ', $node->nodeValue, "\n";
}
echo $html->saveHTML($body->firstChild), "\n";
