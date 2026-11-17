"""Export real native UI surfaces with fictional data, then encode public media.

Development dependency: Pillow and NumPy. These are not application dependencies.
"""
from pathlib import Path
import json
import struct
import subprocess
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageChops

ROOT = Path(__file__).resolve().parent.parent
FRAMES = ROOT / "artifacts/public-demo/frames"
OUTPUT = ROOT / "docs/media"
BACKGROUND = (9, 12, 17)
CAPTIONS = {"idle": "Cursor-following eyes", "compact": "Compact now playing",
            "media": "Media controls", "lyrics": "Timed lyrics",
            "weather": "Weather for your selected city", "timer": "A little time to focus",
            "water": "Water check-ins", "notes": "Quick notes, saved locally",
            "settings": "Make it your own"}


def native_image(path):
    raw = path.read_bytes()
    start = struct.unpack_from("<I", raw, 10)[0]
    width, height = struct.unpack_from("<ii", raw, 18)
    pixels = np.frombuffer(raw[start:], dtype=np.uint8).reshape(abs(height), width, 4).copy()
    if height > 0:
        pixels = pixels[::-1]
    pixels = pixels[:, :, [2, 1, 0, 3]]
    alpha = pixels[:, :, 3:4].astype(np.float32)
    pixels[:, :, :3] = np.minimum(255, pixels[:, :, :3].astype(np.float32) * 255 / np.maximum(alpha, 1)).astype(np.uint8)
    foreground = Image.fromarray(pixels)
    base = Image.new("RGBA", foreground.size, BACKGROUND + (255,))
    return Image.alpha_composite(base, foreground).convert("RGB")


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    executable = ROOT / "target/release/dynamic-island-for-windows.exe"
    subprocess.run([str(executable), "--render-demo", "--offline", "--data-dir",
                    str(ROOT / "artifacts/public-demo/profile"), "--demo-output", str(FRAMES)], check=True, cwd=ROOT)
    manifest = json.loads((FRAMES / "manifest.json").read_text())
    font = ImageFont.truetype(str(ROOT / "assets/fonts/Inter.ttf"), 14)
    frames = []
    for entry in manifest["frames"]:
        surface = native_image(FRAMES / entry["file"])
        stage = Image.new("RGB", (880, 700), BACKGROUND)
        stage.paste(surface, ((stage.width - surface.width) // 2, 16))
        draw = ImageDraw.Draw(stage)
        label = CAPTIONS[entry["scene"]]
        draw.text((stage.width / 2, 666), label, font=font, anchor="mm", fill=(174, 180, 190))
        frames.append(stage)
    # One palette for every frame keeps text and glow stable while animating.
    contact = Image.new("RGB", (440 * 5, 350 * 2), BACKGROUND)
    for index, frame in enumerate(frames[::max(1, len(frames) // 10)][:10]):
        contact.paste(frame.resize((440, 350), Image.Resampling.LANCZOS), (index % 5 * 440, index // 5 * 350))
    palette = contact.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    encoded = [frame.quantize(palette=palette, dither=Image.Dither.FLOYDSTEINBERG) for frame in frames]
    encoded[0].save(OUTPUT / "demo.gif", save_all=True, append_images=encoded[1:],
                    duration=1000 // manifest["fps"], loop=0, optimize=False, disposal=1,
                    background=palette.getpixel((0, 0)))
    for file in (FRAMES / "screenshots").glob("*.bmp"):
        native_image(file).save(OUTPUT / (file.stem + ".png"), optimize=True)
    # Compact overview made solely from exported native surfaces.
    overview = Image.new("RGB", (1040, 620), BACKGROUND)
    media = native_image(FRAMES / "screenshots/lyrics.bmp")
    idle = native_image(FRAMES / "screenshots/idle.bmp")
    overview.paste(idle, ((1040 - idle.width) // 2, 28))
    overview.paste(media, ((1040 - media.width) // 2, 186))
    overview.save(OUTPUT / "overview.png", optimize=True)
    with Image.open(OUTPUT / "demo.gif") as gif:
        if gif.n_frames != len(encoded):
            raise RuntimeError("GIF frame count changed unexpectedly")
        for index in [0, 40, 70, 110, 145, 180, 220, 255, len(encoded) - 1]:
            gif.seek(index)
            if ImageChops.difference(gif.convert("RGB"), encoded[index].convert("RGB")).getbbox():
                raise RuntimeError(f"GIF disposal/composition differs from native frame {index}")
        decoded = []
        for index in [0, 40, 70, 110, 145, 180, 220, 255, gif.n_frames - 1]:
            gif.seek(min(index, gif.n_frames - 1))
            decoded.append(gif.convert("RGB").resize((352, 280)))
        qa = Image.new("RGB", (352 * 3, 280 * 3), BACKGROUND)
        for index, frame in enumerate(decoded):
            qa.paste(frame, (index % 3 * 352, index // 3 * 280))
        qa.save(ROOT / "artifacts/public-demo/gif-contact-sheet.png")
        summary = {"frames": gif.n_frames, "duration_seconds": sum(gif.seek(i) or gif.info.get("duration", 50)
                    for i in range(gif.n_frames)) / 1000, "gif_bytes": (OUTPUT / "demo.gif").stat().st_size,
                   "provenance": manifest["provenance"], "user_data_used": False}
    (ROOT / "artifacts/public-demo/media-check.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
