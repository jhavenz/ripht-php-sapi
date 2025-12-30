<?php

$format = $_GET['format'] ?? 'raw';

switch ($format) {
    case 'png':
        header('Content-Type: image/png');
        $img = imagecreatetruecolor(10, 10);
        $red = imagecolorallocate($img, 255, 0, 0);
        imagefill($img, 0, 0, $red);
        imagepng($img);
        imagedestroy($img);
        break;
        
    case 'null':
        header('Content-Type: application/octet-stream');
        echo "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09";
        echo "\x00\x00\x00\x00";
        echo "\xff\xfe\xfd\xfc";
        break;

    case 'mixed':
        header('Content-Type: application/octet-stream');
        echo "START";
        echo "\x00\x00";
        echo "MIDDLE";
        echo "\xff\xff";
        echo "END";
        break;

    default:
        header('Content-Type: application/octet-stream');
        $bytes = '';
        for ($i = 0; $i < 256; $i++) {
            $bytes .= chr($i);
        }
        echo $bytes;
}
