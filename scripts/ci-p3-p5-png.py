#!/usr/bin/env python3
"""Expose bounded P3/P5 CI snapshots for visual review."""
import base64
import json
from pathlib import Path
import sys

for name in ('p2-action-payload-error-ru.png', 'p3-columns-focus-ru.png', 'p5-options-hints-ru.png'):
    path = Path('docs/screenshots') / name
    if not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
        sys.exit('Missing or oversized P3/P5 image: ' + name)
    data = path.read_bytes()
    if not data.startswith(b'\x89PNG\r\n\x1a\n'):
        sys.exit('Not a PNG: ' + name)
    encoded = base64.b64encode(data).decode('ascii')
    total = (len(encoded) + 5999) // 6000
    for part in range(total):
        print('P3_P5_PNG_CHUNK ' + json.dumps({'name': name, 'part': part,
              'total': total, 'data': encoded[part * 6000:(part + 1) * 6000]}))
