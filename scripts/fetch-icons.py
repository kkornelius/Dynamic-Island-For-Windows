"""Fetch selected free 100px Icons8 PNGs for embedding, never Git redistribution.

Icons8 requires attribution in the app and README. See assets/icons8/README.md.
This is a build-time fetch; the application never downloads its icons at runtime.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import struct
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / 'assets/icons8/manifest.json'
NAMES = {
    'music': 'music', 'weather': 'partly-cloudy-day', 'timer': 'timer',
    'water': 'water', 'notes': 'note', 'settings': 'settings',
    'lyrics': 'speech-bubble', 'play': 'play', 'pause': 'pause',
    'previous': 'skip-to-start', 'next': 'end', 'earbuds': 'earbud-headphones',
    'keyboard': 'keyboard', 'mouse': 'mouse', 'speaker': 'speaker',
    'phone': 'iphone', 'controller': 'controller', 'bluetooth': 'bluetooth',
    'close': 'multiply', 'moon': 'moon-symbol', 'info': 'info',
}


def fetch(item):
    name, slug, color = item
    folder = ROOT / 'assets/icons8'
    folder.mkdir(parents=True, exist_ok=True)
    target = folder / f'{name}-{color}.png'
    url = f'https://img.icons8.com/sf-regular/100/{color}/{slug}.png'
    if target.exists():
        data = target.read_bytes()
    else:
        request = urllib.request.Request(url, headers={'User-Agent': 'DynamicIslandForWindows-icon-build'})
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(128 * 1024)
    if not data.startswith(b'\x89PNG\r\n\x1a\n'):
        raise ValueError('Not a PNG: ' + name)
    width, height = struct.unpack_from('>II', data, 16)
    if not 0 < width <= 100 or not 0 < height <= 100:
        raise ValueError('Unexpected free PNG dimensions: ' + name)
    expected = KNOWN.get(target.name)
    if expected and hashlib.sha256(data).hexdigest() != expected['sha256']:
        raise ValueError('Icon differs from the reviewed manifest; inspect before updating: ' + name)
    if not target.exists():
        target.write_bytes(data)
    return {'name': target.name, 'url': url, 'sha256': hashlib.sha256(data).hexdigest()}


def main():
    global KNOWN
    KNOWN = {item['name']: item for item in json.loads(MANIFEST.read_text(encoding='utf-8'))} if MANIFEST.exists() else {}
    work = [(name, slug, color) for name, slug in NAMES.items() for color in ('FFFFFF', '000000')]
    with ThreadPoolExecutor(max_workers=3) as pool:
        results = list(pool.map(fetch, work))
    # Provenance is safe to commit; downloaded raw graphics remain ignored.
    content = json.dumps(results, indent=2) + '\n'
    if not MANIFEST.exists() or MANIFEST.read_text(encoding='utf-8') != content:
        MANIFEST.write_text(content, encoding='utf-8')
    print(f'Ready: {len(results)} Icons8 SF Regular PNGs; raw files are excluded from Git.')


if __name__ == '__main__':
    main()
