"""Export documentation-sized PNGs from the DrawDock brand masters."""

import argparse
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[4]
SOURCE = Path(__file__).resolve().parents[1] / "assets"
DESTINATION = ROOT / "docs" / "assets" / "brand"
EXPORTS = (
    (Path("github") / "drawdock-github-cover-1280x640.png", "github-cover.png", 1280),
    (Path("logo") / "drawdock-logo-horizontal.png", "logo-light.png", 640),
    (Path("logo") / "drawdock-logo-horizontal-dark-bg.png", "logo-dark.png", 640),
    (Path("mockups") / "menu-01-root.png", "radial-menu-root.png", 760),
    (Path("mockups") / "menu-09-more.png", "radial-menu-more.png", 760),
    (Path("mockups") / "canvas-01-selection.png", "selection-reference.png", 960),
)


def export_image(relative_path: Path, max_width: int) -> Image.Image:
    with Image.open(SOURCE / relative_path) as source:
        image = source.convert("RGBA" if "A" in source.getbands() else "RGB")
    if image.width > max_width:
        height = round(image.height * max_width / image.width)
        image = image.resize((max_width, height), Image.Resampling.LANCZOS)
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Check exports without writing files.")
    args = parser.parse_args()
    if not args.check:
        DESTINATION.mkdir(parents=True, exist_ok=True)
    failures = []
    for source, name, width in EXPORTS:
        image = export_image(source, width)
        target = DESTINATION / name
        if args.check:
            if not target.is_file():
                failures.append(f"Missing export: {name}")
                continue
            with Image.open(target) as actual:
                if actual.size != image.size or actual.convert(image.mode).tobytes() != image.tobytes():
                    failures.append(f"Export differs from its source: {name}")
        else:
            image.save(target, format="PNG", optimize=True)
            print(f"{name}: {image.width}x{image.height}, {target.stat().st_size // 1024} KiB")
    if failures:
        for failure in failures:
            print(failure)
        return 1
    print(f"{len(EXPORTS)} documentation exports {'verified' if args.check else 'written'}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
