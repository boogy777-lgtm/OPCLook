#!/usr/bin/env python3
"""Expose bounded synthetic UI QA images through the CI log for visual review."""
import base64
import json
from pathlib import Path
import sys

names = ['p4-palette-browse-ru.png', 'p4-palette-browse-en.png',
         'p4-palette-browse-zh.png', 'p4-palette-write-confirm-ru.png']
for name in names:
    path = Path('docs/screenshots') / name
    if not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
        sys.exit('Missing or oversized P4 QA image: ' + name)
    data = path.read_bytes()
    if not data.startswith(b'\x89PNG\r\n\x1a\n'):
        sys.exit('Not a PNG: ' + name)
    encoded = base64.b64encode(data).decode('ascii')
    total = (len(encoded) + 5999) // 6000
    for part in range(total):
        print('P4_PNG_CHUNK ' + json.dumps({'name': name, 'part': part, 'total': total,
              'data': encoded[part * 6000:(part + 1) * 6000]}))
