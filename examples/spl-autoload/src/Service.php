<?php
namespace App;

use App\Models\User;

class Service {
    public function welcome(User $u): string {
        return $u->greet() . " from the service layer";
    }
}
