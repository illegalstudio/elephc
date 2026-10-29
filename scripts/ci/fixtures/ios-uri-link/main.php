<?php

$uri = new Uri\Rfc3986\Uri("https://example.com/a");
echo $uri->getHost(), ':', $uri->getPath(), "\n";

#[Export]
function ios_uri_link_smoke(): int {
    return 0;
}
