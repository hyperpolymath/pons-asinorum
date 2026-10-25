<?php
function fail_if($condition) {
    if ($condition) {
        throw new RuntimeException('failed');
    }
    work();
}
