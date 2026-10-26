"""Generate the OPC DA/UA Test Client icon.

Creates a multi-size assets/icon.ico (16..256 px) for the Windows resource
embedded by build.rs, plus a raw 32x32 RGBA blob (assets/icon_32.rgba) used as
the runtime window icon via ViewportBuilder::with_icon.

Design: dark navy-blue rounded square (#1E3A5F) with a white "OPC" monogram.
Requires Pillow and Python 3. Run:  py assets/make_icon.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HD = Path(__file__).parent


def rounded_square_mask(size: int, radius_ratio: float = 0.22) -> Image.Image:
    """Anti-aliased rounded-square alpha mask centered in a `size`x`size canvas."""
    scale = 4
    big = size * scale
    mask = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(mask)
    radius = int(big * radius_ratio)
    d.rounded_rectangle([0, 0, big - 1, big - 1], radius=radius, fill=255)
    return mask.resize((size, size), Image.Resampling.LANCZOS)


def draw_glyph(d: ImageDraw.ImageDraw, size: int, font) -> None:
    """White 'OPC' monogram, vertically centered using the text bounding box."""
    text = "OPC"
    bbox = d.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    x = (size - tw) / 2 - bbox[0]
    y = (size - th) / 2 - bbox[1]
    d.text((x, y), text, font=font, fill=(255, 255, 255, 255))


def make_canvas(size: int) -> Image.Image:
    bg = (30, 58, 95, 255)  # #1E3A5F
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    mask = rounded_square_mask(size)
    solid = Image.new("RGBA", (size, size), bg)
    img.paste(solid, (0, 0), mask)

    # Subtle vertical gradient: darker at bottom for depth.
    grad = Image.new("L", (1, size), 0)
    for y in range(size):
        t = y / (size - 1)
        grad.putpixel((0, y), int(70 * (1 - t)))
    grad = grad.resize((size, size))
    shade = Image.new("RGBA", (size, size), (8, 20, 38, 255))
    img = Image.composite(shade, img, grad.point(lambda p: int(255 * p / 100)))
    img.putalpha(mask)

    # Font size scales with canvas; keep it readable down to 16px.
    font_size = max(10, int(size * 0.46))
    try:
        font = ImageFont.truetype("arialbd.ttf", font_size)
    except Exception:
        font = ImageFont.load_default()
    draw_glyph(ImageDraw.Draw(img), size, font)
    return img


def main() -> None:
    sizes = [16, 32, 48, 64, 128, 256]
    # Render the highest-res frame and let Pillow produce the smaller
    # downscaled variants for the multi-size .ico.
    base = make_canvas(256)
    ico = HD / "icon.ico"
    base.save(ico, format="ICO", sizes=[(s, s) for s in sizes])
    print(f"wrote {ico} ({len(sizes)} sizes)")

    frame32 = make_canvas(32)

    rgba = HD / "icon_32.rgba"
    rgba.write_bytes(frame32.convert("RGBA").tobytes())
    expected = 32 * 32 * 4
    actual = rgba.stat().st_size
    assert actual == expected, f"unexpected size {actual} != {expected}"
    print(f"wrote {rgba} ({actual} bytes, 32x32 RGBA)")


if __name__ == "__main__":
    main()