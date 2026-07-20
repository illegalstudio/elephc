<?php

enum Color: int {
    case Red = 1;
    case Green = 2;
    case Blue = 3;
}

$picked = Color::tryFrom(4) ?? Color::Red;
echo $picked === Color::Red;
echo PHP_EOL;
echo Color::Green->value;
echo PHP_EOL;
echo count(Color::cases());
echo PHP_EOL;

// Every enum case exposes a read-only ->name (the case identifier);
// backed cases also expose ->value.
foreach (Color::cases() as $color) {
    echo $color->name, "=", $color->value, " ";
}
echo PHP_EOL;

// PHP permits keywords as case names. Their source spelling is preserved and
// case-sensitive, so Match and MATCH are two distinct cases.
enum ParserOutcome: string {
    case Default = "default";
    case Match = "match";
    case MATCH = "upper-match";
}

foreach (ParserOutcome::cases() as $outcome) {
    echo $outcome->name, "=", $outcome->value, " ";
}
echo PHP_EOL;

function sql_sort_keyword(SortDirection $direction): string {
    return match ($direction) {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
}

echo sql_sort_keyword(SortDirection::Descending);
