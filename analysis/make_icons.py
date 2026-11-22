"""生成 GamePP-Lite 的图标资源（纯 Python，不依赖 Pillow）。

产出：
  assets/app.ico      未优化态：石板灰圆角方 + 白色闪电
  assets/app-opt.ico  已优化态：绿色圆角方 + 白色对勾

ICO 内部用 32bpp BMP（BITMAPINFOHEADER + BGRA 自下而上 + AND 掩码），
Windows 的 CreateIconFromResourceEx 与资源编译器都能直接吃。
每个尺寸先按 4 倍超采样绘制再缩，边缘才不锯齿。
"""

import os
import struct

SIZES = [16, 24, 32, 48, 64, 128]
SS = 4  # 超采样倍数

SLATE = (0x5A, 0x64, 0x73, 255)
GREEN = (0x1E, 0x8E, 0x5A, 255)
WHITE = (255, 255, 255, 255)

BOLT = [
    (0.58, 0.06), (0.28, 0.56), (0.45, 0.56),
    (0.38, 0.94), (0.72, 0.40), (0.53, 0.40),
]
CHECK = [(0.20, 0.54), (0.42, 0.76), (0.80, 0.26)]


def in_round_rect(x, y, size, radius):
    r = radius
    if x < r and y < r:
        return (x - r) ** 2 + (y - r) ** 2 <= r * r
    if x > size - r and y < r:
        return (x - (size - r)) ** 2 + (y - r) ** 2 <= r * r
    if x < r and y > size - r:
        return (x - r) ** 2 + (y - (size - r)) ** 2 <= r * r
    if x > size - r and y > size - r:
        return (x - (size - r)) ** 2 + (y - (size - r)) ** 2 <= r * r
    return True


def in_polygon(x, y, pts):
    inside = False
    n = len(pts)
    j = n - 1
    for i in range(n):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y):
            xt = (xj - xi) * (y - yi) / (yj - yi) + xi
            if x < xt:
                inside = not inside
        j = i
    return inside


def dist_to_segment(px, py, ax, ay, bx, by):
    vx, vy = bx - ax, by - ay
    wx, wy = px - ax, py - ay
    L = vx * vx + vy * vy
    t = 0.0 if L == 0 else max(0.0, min(1.0, (wx * vx + wy * vy) / L))
    dx, dy = px - (ax + t * vx), py - (ay + t * vy)
    return (dx * dx + dy * dy) ** 0.5


def render(size, bg, glyph):
    s = size * SS
    radius = s * 0.24
    pts = [(x * s, y * s) for x, y in (BOLT if glyph == "bolt" else [])]
    thick = s * 0.15
    seg = [(x * s, y * s) for x, y in CHECK]

    buf = []
    for y in range(s):
        for x in range(s):
            cx, cy = x + 0.5, y + 0.5
            px = (0, 0, 0, 0)
            if in_round_rect(cx, cy, s, radius):
                px = bg
                if glyph == "bolt":
                    if in_polygon(cx, cy, pts):
                        px = WHITE
                else:
                    d = min(
                        dist_to_segment(cx, cy, *seg[0], *seg[1]),
                        dist_to_segment(cx, cy, *seg[1], *seg[2]),
                    )
                    if d <= thick:
                        px = WHITE
            buf.append(px)

    # box 缩采样
    out = []
    for y in range(size):
        for x in range(size):
            rs = gs = bs = as_ = 0
            for dy in range(SS):
                for dx in range(SS):
                    r, g, b, a = buf[(y * SS + dy) * s + (x * SS + dx)]
                    rs += r * a
                    gs += g * a
                    bs += b * a
                    as_ += a
            n = SS * SS
            if as_ == 0:
                out.append((0, 0, 0, 0))
            else:
                out.append((rs // as_, gs // as_, bs // as_, as_ // n))
    return out


def bmp_image(pixels, size):
    """BITMAPINFOHEADER + BGRA(bottom-up) + AND mask。"""
    hdr = struct.pack(
        "<IiiHHIIiiII",
        40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0,
    )
    body = bytearray()
    for y in range(size - 1, -1, -1):
        for x in range(size):
            r, g, b, a = pixels[y * size + x]
            body += bytes((b, g, r, a))
    mask_row = ((size + 31) // 32) * 4
    mask = b"\x00" * (mask_row * size)
    return hdr + bytes(body) + mask


def build_ico(path, bg, glyph):
    images = [(s, bmp_image(render(s, bg, glyph), s)) for s in SIZES]
    out = bytearray(struct.pack("<HHH", 0, 1, len(images)))
    offset = 6 + 16 * len(images)
    for s, data in images:
        out += struct.pack(
            "<BBBBHHII",
            0 if s >= 256 else s,
            0 if s >= 256 else s,
            0, 0, 1, 32, len(data), offset,
        )
        offset += len(data)
    for _, data in images:
        out += data
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(out)
    print(f"{path}  {len(out)} bytes  sizes={SIZES}")


if __name__ == "__main__":
    root = os.path.join(os.path.dirname(__file__), "..", "gamepp-lite", "assets")
    build_ico(os.path.abspath(os.path.join(root, "app.ico")), SLATE, "bolt")
    build_ico(os.path.abspath(os.path.join(root, "app-opt.ico")), GREEN, "check")
