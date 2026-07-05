<?php

header('Content-Type: text/plain');
header('X-Ripht-Sink: yes');

echo 'alpha';
flush();
echo 'omega';
