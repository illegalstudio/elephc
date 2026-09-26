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
echo PHP_EOL;

// An enum case is a constant expression, so it can be the default of a declared
// property, a static property, a promoted constructor property, or a parameter.
// Every form stores the canonical singleton, so === against the case holds.
enum Level {
    case Low;
    case High;
}

class Config {
    public static Level $shared = Level::High;
    public Level $level = Level::Low;

    public function __construct(
        public Level $promoted = Level::High,
    ) {}
}

$config = new Config();
echo $config->level->name, " ",
     $config->promoted->name, " ",
     Config::$shared->name, " ",
     ($config->level === Level::Low ? "same" : "DIFF");
echo PHP_EOL;

// Every enum implements UnitEnum, and a backed one also implements BackedEnum,
// without saying so in its declaration. That lets code accept "any enum"
// without naming each one.
function describe(object $value): string {
    if ($value instanceof BackedEnum) {
        return "backed";
    }
    if ($value instanceof UnitEnum) {
        return "pure";
    }
    return "not an enum";
}

echo describe(Color::Red), " ", describe(Level::Low), " ", describe($config);
echo PHP_EOL;
