"""Draw the Hihu Island icon: an amber tile, a black notch pill like bangs,
and two happy closed eyes (^ ^) with blush. Small sizes get a simpler face."""
import sys
from pathlib import Path
from PIL import Image, ImageDraw

AMBER = (245, 185, 66, 255)
INK = (20, 20, 20, 255)
BLUSH = (255, 128, 112, 200)
SS = 8  # supersampling


def draw(size: int) -> Image.Image:
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    small = size <= 24
    m = 0 if small else round(0.03 * s)
    d.rounded_rectangle([m, m, s - 1 - m, s - 1 - m], radius=round(0.24 * s), fill=AMBER)

    def box(x0, y0, x1, y1):
        return [round(x0 * s), round(y0 * s), round(x1 * s), round(y1 * s)]

    def caret(cx, top, half, h, w):
        pts = [(round((cx - half) * s), round((top + h) * s)), (round(cx * s), round(top * s)),
               (round((cx + half) * s), round((top + h) * s))]
        d.line(pts, fill=INK, width=round(w * s), joint="curve")
        r = w * s / 2
        for x, y in (pts[0], pts[2]):
            d.ellipse([x - r, y - r, x + r, y + r], fill=INK)

    if small:
        d.rounded_rectangle(box(0.12, 0.10, 0.88, 0.42), radius=round(0.16 * s), fill=INK)
        for cx in (0.31, 0.69):
            caret(cx, 0.56, 0.14, 0.14, 0.12)
    else:
        d.rounded_rectangle(box(0.20, 0.14, 0.80, 0.36), radius=round(0.11 * s), fill=INK)
        for cx in (0.35, 0.65):
            caret(cx, 0.50, 0.085, 0.085, 0.05)
        for cx in (0.24, 0.76):
            d.ellipse(box(cx - 0.065, 0.66, cx + 0.065, 0.72), fill=BLUSH)
        d.arc(box(0.44, 0.64, 0.56, 0.76), 20, 160, fill=INK, width=round(0.035 * s))
    return img.resize((size, size), Image.LANCZOS)


def main(out: Path):
    out.mkdir(parents=True, exist_ok=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 256]
    frames = [draw(n) for n in sizes]
    frames[-1].save(out / "hihu.ico", sizes=[(n, n) for n in sizes], append_images=frames[:-1])
    draw(1024).save(out / "hihu-1024.png")
    # Preview sheet: every size at 1:1 on light and dark, plus the big one.
    sheet = Image.new("RGBA", (700, 380), (0, 0, 0, 0))
    for row, bg in enumerate([(243, 243, 243, 255), (32, 32, 32, 255)]):
        y0 = 260 + row * 60
        Image.Image.paste(sheet, Image.new("RGBA", (700, 60), bg), (0, y0))
        x = 20
        for f in frames[:-1]:
            sheet.alpha_composite(f, (x, y0 + (60 - f.size[1]) // 2))
            x += f.size[0] + 24
    sheet.alpha_composite(draw(240), (20, 10))
    sheet.alpha_composite(draw(240).resize((120, 120), Image.LANCZOS), (300, 70))
    sheet.save(out / "hihu-preview.png")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
