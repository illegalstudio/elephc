<?php

$uri = new Uri\Rfc3986\Uri(
    "HTTPS://user:pass@EXAMPLE.com:0443/a/../b?x=%7e#f"
);

echo "normalized: ", $uri->toString(), "\n";
echo "raw:        ", $uri->toRawString(), "\n";
echo "path:       ", $uri->getPath(), "\n";
echo "new path:   ", $uri->withPath("/new")->toString(), "\n";

$url = new Uri\WhatWg\Url("https://user:pass@example.com:443/a/../b?q=1#f");
echo "ascii:      ", $url->toAsciiString(), "\n";
echo "unicode:    ", $url->toUnicodeString(), "\n";

$errors = [];
$resolved = $url->resolve(" /next", $errors);
echo "resolved:   ", $resolved->toAsciiString(), "\n";
foreach ($errors as $error) {
    echo "warning:    ", $error->type->name, " (", $error->context, ")\n";
}

try {
    new Uri\WhatWg\Url("http://localhost:99999");
} catch (Uri\WhatWg\InvalidUrlException $exception) {
    echo "error:      ", $exception->getMessage(), "\n";
}
