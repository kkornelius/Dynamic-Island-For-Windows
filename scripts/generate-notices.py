"""Regenerate the license notices embedded in the single-file executable."""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"], cwd=ROOT, text=True
    ))
    sections = [
        "DYNAMIC ISLAND FOR WINDOWS — LICENSE NOTICES\n",
        "Project license\n\n" + (ROOT / "LICENSE").read_text(encoding="utf-8"),
        "Embedded Inter font\n\n" + (ROOT / "assets/fonts/OFL.txt").read_text(encoding="utf-8"),
        "Embedded water sound\n\n" + (ROOT / "assets/sounds/README.md").read_text(encoding="utf-8"),
        "Embedded Icons8 graphics (separate license; not MIT)\n\n" + (ROOT / "assets/icons8/README.md").read_text(encoding="utf-8"),
    ]
    for package in sorted(metadata["packages"], key=lambda p: p["name"]):
        if package["name"] == "dynamic-island-for-windows":
            continue
        folder = Path(package["manifest_path"]).parent
        title = f'{package["name"]} {package["version"]} — {package.get("license", "see upstream")}'
        texts = []
        for path in sorted(folder.iterdir()):
            if path.is_file() and path.name.upper().split(".")[0].split("-")[0] in {
                "LICENSE", "LICENCE", "COPYING", "NOTICE"
            }:
                texts.append(path.name + "\n\n" + path.read_text(encoding="utf-8", errors="replace"))
        if not texts:
            raise SystemExit(f"No license text found for {package['name']}")
        sections.append(title + "\n\n" + "\n\n".join(texts))
    target = ROOT / "assets/licenses/NOTICES.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    content = "\n\n" + ("\n\n" + "=" * 72 + "\n\n").join(sections)
    if not target.exists() or target.read_text(encoding="utf-8") != content:
        target.write_text(content, encoding="utf-8")
    print(f"Generated {target.relative_to(ROOT)} ({target.stat().st_size:,} bytes)")


if __name__ == "__main__":
    main()
