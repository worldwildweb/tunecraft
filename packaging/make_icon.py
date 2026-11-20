"""Draws the TuneCraft app icon (original artwork): a blue rounded tile with a white
double note. Writes assets/app-icon/tunecraft-1024.png and, on macOS, tunecraft.icns."""
import os, subprocess, shutil, tempfile
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "assets", "app-icon")
os.makedirs(OUT, exist_ok=True)

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
m = 100  # macOS icon grid margin
d.rounded_rectangle([m, m, S - m, S - m], radius=185, fill=(0x4E, 0x7B, 0xFB, 255))
w = (255, 255, 255, 255)
# two note heads, stems and a beam
d.ellipse([300, 600, 450, 720], fill=w)
d.ellipse([560, 540, 710, 660], fill=w)
d.rectangle([420, 330, 450, 665], fill=w)
d.rectangle([680, 290, 710, 605], fill=w)
d.polygon([(420, 330), (710, 270), (710, 340), (420, 400)], fill=w)
png = os.path.join(OUT, "tunecraft-1024.png")
img.save(png)

if shutil.which("iconutil"):
    tmp = tempfile.mkdtemp()
    iconset = os.path.join(tmp, "tunecraft.iconset")
    os.makedirs(iconset)
    for size in (16, 32, 128, 256, 512):
        img.resize((size, size), Image.LANCZOS).save(os.path.join(iconset, f"icon_{size}x{size}.png"))
        img.resize((size * 2, size * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{size}x{size}@2x.png"))
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(OUT, "tunecraft.icns")], check=True)
print("icon written to", OUT)
