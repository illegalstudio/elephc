<?php

namespace Demo\Support;

use Demo\Domain\User;

const APP_ENV = "dev";

function format_user(User $user) {
    return "[" . $user->badge() . "]";
}
