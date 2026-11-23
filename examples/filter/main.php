<?php

$age = filter_var("42", FILTER_VALIDATE_INT, [
    "options" => ["min_range" => 18, "max_range" => 120],
]);
var_dump($age);

$email = filter_var("ada@example.com", FILTER_VALIDATE_EMAIL);
var_dump($email);

$unsafe = "<b>Tom & Jerry</b>";
var_dump(filter_var($unsafe, FILTER_SANITIZE_SPECIAL_CHARS));

$record = filter_var_array(
    ["name" => "Ada", "age" => "36", "score" => "12.5"],
    [
        "name" => FILTER_SANITIZE_SPECIAL_CHARS,
        "age" => FILTER_VALIDATE_INT,
        "score" => FILTER_VALIDATE_FLOAT,
    ]
);
var_dump($record);
