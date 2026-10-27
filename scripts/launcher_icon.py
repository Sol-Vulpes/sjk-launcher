"""The SJK Launcher's icon: a gold play mark in a gold ring on SJK's navy, the
launcher's Play button as a badge. Drawn here so it can be made again at any size.

    python scripts/launcher_icon.py assets/launcher-icon.png [size]

Needs Pillow. The launcher's window and tray use it, and SJK shows it before SJK chat
messages sent from the launcher (SJK's assets/branding/sjk-launcher-icon.png, 64 px).
"""

import sys

from PIL import Image, ImageDraw

SPACE = (0x06, 0x0A, 0x14)
SKY = (0x12, 0x22, 0x44)
GOLD = (0xE8, 0xB8, 0x4A)
GOLD_BRIGHT = (0xFF, 0xD9, 0x7A)
HOLO = (0xA8, 0xCF, 0xFF)
SUPERSAMPLE = 4


def mix(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def vertical(size, top, bottom):
    """A square filled from `top` to `bottom`."""
    image = Image.new("RGBA", (size, size))
    draw = ImageDraw.Draw(image)
    for y in range(size):
        draw.line([(0, y), (size, y)], fill=mix(top, bottom, y / (size - 1)) + (255,))
    return image


def circle(size, radius):
    """A mask of a centred disc, `radius` a fraction of the side."""
    mask = Image.new("L", (size, size), 0)
    centre, r = size / 2, radius * size
    ImageDraw.Draw(mask).ellipse([centre - r, centre - r, centre + r, centre + r], fill=255)
    return mask


def icon(size):
    big = size * SUPERSAMPLE
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    # The ground: a navy disc, lighter towards its top as SJK's sky is.
    out.paste(vertical(big, SKY, SPACE), mask=circle(big, 0.47))
    # The ring: gold, brighter at the top.
    ring = circle(big, 0.47)
    ring.paste(0, mask=circle(big, 0.415))
    out.paste(vertical(big, GOLD_BRIGHT, GOLD), mask=ring)
    # A thin holo line inside it.
    line = circle(big, 0.372)
    line.paste(0, mask=circle(big, 0.362))
    out.paste(Image.new("RGBA", (big, big), HOLO + (255,)), mask=line.point(lambda v: v * 90 // 255))
    # The play mark, set a little right of centre so it looks centred.
    play = Image.new("L", (big, big), 0)
    points = [(0.405, 0.305), (0.405, 0.695), (0.725, 0.5)]
    ImageDraw.Draw(play).polygon([(x * big, y * big) for x, y in points], fill=255)
    out.paste(vertical(big, GOLD_BRIGHT, GOLD), mask=play)
    return out.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    icon(int(sys.argv[2]) if len(sys.argv) > 2 else 512).save(sys.argv[1])
