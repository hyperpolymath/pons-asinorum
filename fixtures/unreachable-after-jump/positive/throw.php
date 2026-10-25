<?php
function fail() {
    throw new RuntimeException('failed');
    work();
}
