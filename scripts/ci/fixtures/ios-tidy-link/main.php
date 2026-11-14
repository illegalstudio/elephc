<?php
$tidy = tidy_parse_string('<B>hello</I>', ['show-body-only' => true]);
echo $tidy;

#[Export]
function ios_tidy_link_smoke(): int {
    return 0;
}
