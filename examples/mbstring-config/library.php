<?php
// The host selects the initial encoding with the compiler's --ini option.
#[Export]
function label_length(string $label): int {
    return mb_strlen($label);
}

#[Export]
function label_encoding(): string {
    return (string)mb_internal_encoding();
}
