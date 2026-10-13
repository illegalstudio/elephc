<?php

class ReportLabel
{
    public function __construct(private string $text) {}

    public function __toString(): string
    {
        return $this->text;
    }
}

class Report
{
    public string $title = "";
}

function label(int $version): mixed
{
    return new ReportLabel("Draft " . $version);
}

function updateTitle(mixed $report, string $field, int $version): void
{
    $report->$field = label($version);
}

$report = new Report();
for ($version = 1; $version <= 3; $version++) {
    updateTitle($report, "title", $version);
    echo $report->title, "\n";
}
