<?php

function sample($value, $fallback) {
    if ($value > 0) {
        return $value;
    }
    return $fallback;
}
