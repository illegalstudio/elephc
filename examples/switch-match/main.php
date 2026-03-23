<?php
// Switch and match expressions

$day = 3;

// Switch with fall-through
switch ($day) {
    case 1:
        echo "Monday\n";
        break;
    case 2:
        echo "Tuesday\n";
        break;
    case 3:
        echo "Wednesday\n";
        break;
    case 4:
        echo "Thursday\n";
        break;
    case 5:
        echo "Friday\n";
        break;
    default:
        echo "Weekend\n";
        break;
}

// Match expression (PHP 8 style)
$status = 404;
$message = match($status) {
    200 => "OK",
    301 => "Moved",
    404 => "Not Found",
    500 => "Server Error",
    default => "Unknown",
};
echo "HTTP " . $status . ": " . $message . "\n";

// Switch with string comparison
$color = "green";
switch ($color) {
    case "red":
        echo "Stop\n";
        break;
    case "yellow":
        echo "Caution\n";
        break;
    case "green":
        echo "Go\n";
        break;
}

// Switch fall-through (grouping cases)
$grade = 85;
$letter = "?";
if ($grade >= 90) {
    $letter = "A";
} elseif ($grade >= 80) {
    $letter = "B";
} elseif ($grade >= 70) {
    $letter = "C";
} else {
    $letter = "F";
}
echo "Grade: " . $letter . "\n";
