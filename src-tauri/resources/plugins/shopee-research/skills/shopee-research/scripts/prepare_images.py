"""Normalize cropped CUA product screenshots for compact research workbooks."""
import argparse
import io
import json
import re
from pathlib import Path

from PIL import Image, ImageOps


def prepare(source, destination, size=320, limit=40 * 1024):
    with Image.open(source) as original:
        image = ImageOps.exif_transpose(original).convert("RGBA")
        image.thumbnail((size - 16, size - 16), Image.Resampling.LANCZOS)
        canvas = Image.new("RGB", (size, size), "white")
        canvas.paste(image, ((size - image.width) // 2, (size - image.height) // 2), image)
        for quality in (78, 70, 62, 54, 46):
            buffer = io.BytesIO()
            canvas.save(buffer, "JPEG", quality=quality, optimize=True)
            data = buffer.getvalue()
            if len(data) <= limit:
                break
        if len(data) > limit:
            raise ValueError(f"Image exceeds {limit} bytes after compression: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(data)
    return len(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="JSON list: sheet, cell, productId, path")
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    entries = json.loads(args.manifest.read_text())
    total = 0
    prepared = {}
    for entry in entries:
        product_id = str(entry["productId"])
        if not re.fullmatch(r"[A-Za-z0-9_-]+", product_id):
            raise ValueError(f"Invalid product ID: {product_id}")
        source = Path(entry["path"])
        if not source.is_absolute():
            source = args.manifest.parent / source
        target = args.output_dir.resolve() / (product_id + ".jpg")
        if product_id in prepared and prepared[product_id] != source.resolve():
            raise ValueError(f"Conflicting screenshot paths for {product_id}")
        if product_id not in prepared:
            total += prepare(source, target)
            prepared[product_id] = source.resolve()
        entry["path"] = str(target)
    output = args.output_dir / "images.json"
    output.write_text(json.dumps(entries, ensure_ascii=False, indent=2))
    print(json.dumps({"manifest": str(output), "images": len(entries), "imageBytes": total}))


if __name__ == "__main__":
    main()
