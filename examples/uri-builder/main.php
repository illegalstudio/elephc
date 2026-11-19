<?php
// PHP 8.6 syntax: compile with --php-version=8.6.

$uri = new Uri\Rfc3986\UriBuilder()
    ->setScheme("https")
    ->setUserInfo("user:pass")
    ->setHost("example.com")
    ->setPort(8443)
    ->setPath("/search")
    ->setQuery("q=uri+builder")
    ->build();

echo $uri->toRawString(), "\n";
echo $uri->getUriType()->name, " / ", $uri->getHostType()->name, "\n";

$base = new Uri\WhatWg\Url("https://example.com/api/v1/items");
$url = new Uri\WhatWg\UrlBuilder()
    ->setPath("items/42")
    ->setFragment("details")
    ->build($base);

echo $url->toAsciiString(), "\n";
var_dump($url->isSpecialScheme(), $url->getHostType()->name);

use Uri\WhatWg\UrlPercentEncodingMode as Mode;
use function Uri\WhatWg\url_percent_encode;

echo url_percent_encode("a b/c", Mode::PathSegment), "\n";
