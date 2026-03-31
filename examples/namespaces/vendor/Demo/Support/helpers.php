<?php

namespace Demo\Support;

const APP_ENV = "dev";

function format_user($user) {
    return "[" . $user->badge() . "]";
}
