"""Check reviewed public paths, credential patterns, and relative doc links.

This release guard supplements manual review; it cannot prove absence of secrets.
Reports never print matched credential values.
"""
import argparse
import html
import re
import subprocess
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
ROOT_FILES = {
    '.gitignore', '.gitattributes', 'Cargo.toml', 'Cargo.lock', 'LICENSE',
    'README.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'SECURITY.md',
    '.cargo/config.toml', '.github/workflows/ci.yml',
    '.github/ISSUE_TEMPLATE/bug_report.yml', '.github/ISSUE_TEMPLATE/config.yml',
}
DOCS = {f'docs/{name}.md' for name in (
    'ARCHITECTURE', 'BUILDING', 'USAGE', 'PRIVACY', 'PERFORMANCE',
    'RELEASING', 'PUBLIC-FILES',
)}
MEDIA = {f'docs/media/{name}' for name in (
    'overview.png', 'demo.gif', 'idle.png', 'compact.png', 'media.png',
    'lyrics.png', 'weather.png', 'timer.png', 'water.png', 'notes.png', 'settings.png',
)}
ASSETS = {
    'assets/fonts/Inter.ttf', 'assets/fonts/OFL.txt',
    'assets/sounds/water-gulp.wav', 'assets/sounds/README.md',
    'assets/licenses/NOTICES.txt',
}
SCRIPTS = {f'scripts/{name}' for name in (
    'verify.ps1', 'package.ps1', 'measure-performance.ps1', 'generate-notices.py',
    'render-demo.py', 'verify-portable.py', 'audit-public.py',
)}
EXACT = ROOT_FILES | DOCS | MEDIA | ASSETS | SCRIPTS
BINARY = {'.png', '.gif', '.ttf', '.wav'}
PATTERNS = {
    'GitHub credential': r'(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,}',
    'OpenAI credential': r'sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{40,}',
    'AWS access key': r'(?:AKIA|ASIA)[A-Z0-9]{16}',
    'private key': r'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----',
    'credential URL': r'https?://[^\s/:]+:[^\s/@]+@',
    'personal Windows path': r'(?i)[a-z]:[\\/]+Users[\\/]+[^\\/\s"<>]+',
    'credential assignment': r'''(?i)(?:api[_-]?key|access[_-]?token|password|client[_-]?secret)\s*[:=]\s*["'][A-Za-z0-9_./+\-=]{20,}["']''',
}


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def allowed(name):
    path = PurePosixPath(name)
    return name in EXACT or (
        path.parts[0] == 'src' and path.suffix == '.rs' and '..' not in path.parts
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--staged', action='store_true')
    mode.add_argument('--tracked', action='store_true')
    args = parser.parse_args()
    # Check the entire index, including unchanged files from earlier commits.
    names = [p.decode('utf-8') for p in git('ls-files', '-z').split(b'\0') if p]
    if not names:
        raise SystemExit('Nothing staged/tracked. Stage the reviewed allowlist first.')
    failures = []
    scanned_bytes = 0
    for name in names:
        if not allowed(name):
            failures.append(f'{name}: outside the reviewed public allowlist')
            continue
        data = git('show', ':' + name) if args.staged else (ROOT / name).read_bytes()
        scanned_bytes += len(data)
        if len(data) > 10 * 1024 * 1024:
            failures.append(f'{name}: exceeds 10 MiB review limit')
        if PurePosixPath(name).suffix in BINARY:
            continue
        text = data.decode('utf-8', errors='strict')
        for label, pattern in PATTERNS.items():
            for match in re.finditer(pattern, text):
                line = text.count('\n', 0, match.start()) + 1
                failures.append(f'{name}:{line}: potential {label}')
        if not name.endswith('.md'):
            continue
        links = re.findall(r'!?\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)', text)
        links += re.findall(r'(?:src|href)=["\']([^"\']+)["\']', text)
        for link in links:
            link = html.unescape(link.strip('<>'))
            parsed = urlsplit(link)
            if parsed.scheme or link.startswith('#') or not parsed.path:
                continue
            target = (ROOT / PurePosixPath(name).parent / unquote(parsed.path)).resolve()
            if not target.is_relative_to(ROOT) or target.relative_to(ROOT).as_posix() not in names:
                failures.append(f'{name}: relative link missing from public index: {link}')
    if failures:
        print('\n'.join(failures))
        raise SystemExit(f'Public audit failed: {len(failures)} issue(s).')
    print(f'Public audit passed: {len(names)} files, {scanned_bytes:,} bytes; allowlist, credential patterns and relative links checked.')


if __name__ == '__main__':
    main()
