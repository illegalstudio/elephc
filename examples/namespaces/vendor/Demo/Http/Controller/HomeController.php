<?php

namespace Demo\Http\Controller;

use Demo\Domain\User;
use Demo\View\HtmlRenderer;

class HomeController {
    public function index(User $user) {
        $renderer = new HtmlRenderer();
        return $renderer->page("Dashboard", $user->badge());
    }
}
