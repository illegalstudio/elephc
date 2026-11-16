<?php
echo gmp_strval(gmp_add(gmp_init(21), 21)), "\n";

#[Export]
function ios_gmp_link_smoke(): int {
    return 0;
}
