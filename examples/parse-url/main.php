<?php

$url = "https://alice:secret@example.com:8443/products?id=42#details";
$parts = parse_url($url);

echo "Host: ", parse_url($url, PHP_URL_HOST), "\n";
echo "Port: ", parse_url($url, PHP_URL_PORT), "\n";
echo "Path: ", $parts["path"], "\n";
echo "Query: ", $parts["query"], "\n";

$relative = parse_url("//cdn.example.com/assets/app.js");
echo "Scheme-relative host: ", $relative["host"], "\n";

// Build a query string back from structured data.
$query = http_build_query([
    "q" => "elephc compiler",
    "page" => 2,
    "filters" => ["lang" => "php", "tags" => ["aot", "native"]],
    "draft" => null,
]);
echo "Built query: ", $query, "\n";
echo "RFC 3986: ", http_build_query(["q" => "a b~"], "", "&", PHP_QUERY_RFC3986), "\n";
