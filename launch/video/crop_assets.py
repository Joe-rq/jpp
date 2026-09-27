"""把 Codex 生成的表情表、图标表按透明间隙切成单个 PNG（assets/cut/）。"""
from pathlib import Path
from PIL import Image

HERE = Path(__file__).resolve().parent / "assets"
OUT = HERE / "cut"
OUT.mkdir(exist_ok=True)


def runs(profile, min_gap):
    """非空区间列表：profile[i] 为该列/行是否有像素。"""
    out, start, gap = [], None, 0
    for i, v in enumerate(profile + [False] * (min_gap + 1)):
        if v:
            if start is None:
                start = i
            gap = 0
        elif start is not None:
            gap += 1
            if gap > min_gap:
                out.append((start, i - gap + 1))
                start, gap = None, 0
    return out


def split(name, rows, prefix, size=360):
    im = Image.open(HERE / f"{name}.png").convert("RGBA")
    a = im.split()[3].point(lambda v: 255 if v > 24 else 0)
    W, H = im.size
    k = 0
    # 先按行切，再在每行里按列切
    rowprof = [any(a.getpixel((x, y)) for x in range(0, W, 3)) for y in range(H)]
    rr = [r for r in runs(rowprof, 12) if r[1] - r[0] > 40][:rows]
    for y0, y1 in rr:
        band = a.crop((0, y0, W, y1))
        colprof = [any(band.getpixel((x, y)) for y in range(0, y1 - y0, 3)) for x in range(W)]
        for x0, x1 in [c for c in runs(colprof, 14) if c[1] - c[0] > 40]:
            piece = im.crop((x0, y0, x1, y1))
            bb = piece.split()[3].point(lambda v: 255 if v > 24 else 0).getbbox()
            piece = piece.crop(bb)
            s = size / max(piece.size)
            piece = piece.resize((round(piece.size[0] * s), round(piece.size[1] * s)), Image.LANCZOS)
            piece.save(OUT / f"{prefix}-{k}.png")
            k += 1
    print(name, "->", k, "pieces")


def grid(name, cols, rows, prefix, size, inset=0.0):
    """素材彼此贴得太近或单件本身分成几块时，按等分格子切；inset 按格宽比例去掉两侧邻居的边角。"""
    im = Image.open(HERE / f"{name}.png").convert("RGBA")
    W, H = im.size
    k = 0
    for r in range(rows):
        for c in range(cols):
            cw = W // cols
            dx = int(cw * inset)
            piece = im.crop((c * cw + dx, r * H // rows, (c + 1) * cw - dx, (r + 1) * H // rows))
            piece = piece.crop(piece.split()[3].point(lambda v: 255 if v > 24 else 0).getbbox())
            s = size / max(piece.size)
            piece.resize((round(piece.size[0] * s), round(piece.size[1] * s)), Image.LANCZOS).save(OUT / f"{prefix}-{k}.png")
            k += 1
    print(name, "->", k, "pieces (grid)")


for f in OUT.glob("*.png"):
    f.unlink()
grid("char-dev", 3, 1, "dev", 420, 0.045)
split("char-owner", 1, "owner", 420)
grid("icons-eight", 4, 2, "thing", 240)
split("icons-five", 1, "cap", 240)
