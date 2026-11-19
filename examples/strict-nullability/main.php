<?php
declare(strict_types=0);

function optionalName(bool $present): mixed {
    return $present ? 'Ada' : null;
}

function requiredName(bool $present): string {
    return optionalName($present);
}

function displayName(?string $name): string {
    return $name ?? 'anonymous';
}

echo displayName(null), "\n", requiredName(true), "\n";
try {
    echo requiredName(false);
} catch (TypeError $error) {
    echo "A name is required\n";
}
