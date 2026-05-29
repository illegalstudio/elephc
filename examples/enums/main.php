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

function sql_sort_keyword(SortDirection $direction): string {
    return match ($direction) {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
}

echo sql_sort_keyword(SortDirection::Descending);
