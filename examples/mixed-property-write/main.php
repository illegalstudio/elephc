<?php
class Profile {
    public string $name = "unknown";
}

function renameProfile(mixed $profile, string $name): void {
    $profile->name = $name;
}

$profile = new Profile();
renameProfile($profile, "Ada");
echo $profile->name, "\n";
