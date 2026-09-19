<?php
$scores = [10, 20, 30];

foreach ($scores as &$score) {
    $score += 5;
}

foreach ($scores as $index => $value) {
    echo $index . ": " . $value . "\n";
}

// The source can be any array the program can name, not just a local. The writes land in
// the storage the expression reaches, whatever it takes to get there.
class Team {
    public array $scores = [1, 2, 3];
}

// ... through a property of an object held in an array element.
$teams = [new Team()];
foreach ($teams[0]->scores as &$value) {
    $value *= 10;
}
unset($value);
echo "element receiver: " . implode(",", $teams[0]->scores) . "\n";

// ... through a property named at run time.
$team = new Team();
$field = 'scores';
foreach ($team->$field as &$value) {
    $value *= 10;
}
unset($value);
echo "runtime-named property: " . implode(",", $team->scores) . "\n";

// ... through a call result. The loop holds the object for as long as it runs, so the
// writes reach the property the method handed back.
class League {
    private Team $team;
    public function __construct() { $this->team = new Team(); }
    public function team(): Team { return $this->team; }
}

$league = new League();
foreach ($league->team()->scores as &$value) {
    $value *= 10;
}
unset($value);
echo "call-result receiver: " . implode(",", $league->team()->scores) . "\n";

// A by-VALUE loop over the same sources iterates a copy, exactly as PHP does.
foreach ($teams[0]->scores as $value) {
    $value *= 10;
}
echo "by value leaves it alone: " . implode(",", $teams[0]->scores) . "\n";
