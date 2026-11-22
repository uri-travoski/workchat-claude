"""Sample photos, screenshots, avatars and channel pictures drawn with Pillow (no downloads, no copyrighted material)."""
import math
import random
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT_DIRS = ["/usr/share/fonts/truetype/liberation", "/usr/local/share/fonts/truetype/liberation", "/usr/share/fonts/truetype/dejavu"]


def font(size, bold=False):
    names = ["LiberationSans-Bold.ttf", "DejaVuSans-Bold.ttf"] if bold else ["LiberationSans-Regular.ttf", "DejaVuSans.ttf"]
    for d in FONT_DIRS:
        for n in names:
            try:
                return ImageFont.truetype(f"{d}/{n}", size)
            except OSError:
                pass
    return ImageFont.load_default()


def mono(size):
    for d in FONT_DIRS:
        for n in ("LiberationMono-Bold.ttf", "DejaVuSansMono.ttf"):
            try:
                return ImageFont.truetype(f"{d}/{n}", size)
            except OSError:
                pass
    return ImageFont.load_default()


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def vgradient(w, h, stops):
    """Vertical gradient through colour stops [(pos, rgb), ...]."""
    img = Image.new("RGB", (w, h))
    px = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
            if p0 <= t <= p1:
                px.line([(0, y), (w, y)], fill=lerp(c0, c1, (t - p0) / max(1e-9, p1 - p0)))
                break
    return img


def hills(d, w, base, amp, color, seed, step=8, freq=1.0):
    rnd = random.Random(seed)
    ph = [rnd.random() * 6.28 for _ in range(3)]
    pts = [(0, w * 3)]
    for x in range(0, w + step, step):
        y = base + amp * (0.55 * math.sin(x / w * 3.1 * freq + ph[0]) + 0.3 * math.sin(x / w * 7.3 * freq + ph[1]) + 0.15 * math.sin(x / w * 17 * freq + ph[2]))
        pts.append((x, y))
    pts.append((w, w * 3))
    d.polygon(pts, fill=color)


def harbour_sunset(w=1600, h=1067):
    img = vgradient(w, h, [(0, (38, 36, 96)), (0.35, (180, 72, 110)), (0.55, (252, 150, 90)), (0.62, (255, 205, 130)), (1, (255, 220, 160))])
    glow = Image.new("RGB", (w, h), (0, 0, 0))
    gd = ImageDraw.Draw(glow)
    cx, cy = int(w * 0.62), int(h * 0.58)
    for r, a in ((360, 40), (240, 80), (140, 140), (80, 255)):
        gd.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(a, int(a * 0.7), int(a * 0.35)))
    glow = glow.filter(ImageFilter.GaussianBlur(60))
    from PIL import ImageChops
    img = ImageChops.add(img, glow)
    d = ImageDraw.Draw(img)
    d.ellipse([cx - 62, cy - 62, cx + 62, cy + 62], fill=(255, 240, 200))
    hills(d, w, h * 0.60, 60, (70, 52, 92), 1, freq=0.8)
    hills(d, w, h * 0.64, 40, (46, 38, 78), 2, freq=1.4)
    sea = vgradient(w, int(h * 0.38), [(0, (96, 80, 130)), (0.4, (60, 62, 120)), (1, (26, 30, 70))])
    img.paste(sea, (0, int(h * 0.64)))
    d = ImageDraw.Draw(img)
    rnd = random.Random(5)
    for i in range(160):  # sun glitter on the water
        y = int(h * 0.65) + int(rnd.random() ** 1.6 * h * 0.33)
        wd = int(10 + (y - h * 0.65) * 0.5 * rnd.random())
        x = cx + int(rnd.gauss(0, 30 + (y - h * 0.65) * 0.25))
        d.line([(x - wd, y), (x + wd, y)], fill=(255, 214, 150), width=2)
    for bx, s in ((int(w * 0.18), 1.0), (int(w * 0.30), 0.7)):  # small sailing boats
        y = int(h * 0.74)
        d.polygon([(bx, y), (bx + 80 * s, y), (bx + 60 * s, y + 14 * s), (bx + 10 * s, y + 14 * s)], fill=(20, 18, 40))
        d.polygon([(bx + 40 * s, y - 90 * s), (bx + 40 * s, y - 4 * s), (bx + 8 * s, y - 4 * s)], fill=(240, 220, 200))
    return img


def offsite_lake(w=2000, h=1333):
    img = vgradient(w, h, [(0, (110, 170, 230)), (0.5, (190, 225, 250)), (1, (230, 240, 250))])
    d = ImageDraw.Draw(img)
    rnd = random.Random(3)
    for _ in range(9):  # clouds
        cx, cy = rnd.randint(0, w), rnd.randint(40, int(h * 0.3))
        for _ in range(7):
            r = rnd.randint(40, 110)
            ox, oy = rnd.randint(-160, 160), rnd.randint(-25, 25)
            d.ellipse([cx + ox - r, cy + oy - r // 2, cx + ox + r, cy + oy + r // 2], fill=(255, 255, 255))
    img = img.filter(ImageFilter.GaussianBlur(6))
    d = ImageDraw.Draw(img)
    hills(d, w, h * 0.46, 90, (120, 150, 170), 11, freq=0.7)
    hills(d, w, h * 0.52, 70, (70, 112, 96), 12, freq=1.1)
    hills(d, w, h * 0.57, 50, (44, 90, 66), 13, freq=1.6)
    lake = vgradient(w, int(h * 0.44), [(0, (70, 130, 160)), (1, (30, 80, 120))])
    img.paste(lake, (0, int(h * 0.56)))
    d = ImageDraw.Draw(img)
    for i in range(60):  # ripples
        y = int(h * 0.58) + int(rnd.random() * h * 0.4)
        x = rnd.randint(0, w)
        d.line([(x, y), (x + rnd.randint(40, 200), y)], fill=(150, 200, 220), width=2)
    for i in range(46):  # pine trees on the shore
        x = rnd.randint(0, w)
        y = int(h * 0.57) + rnd.randint(-8, 14)
        s = rnd.uniform(0.7, 1.7)
        for k in range(4):
            d.polygon([(x - 34 * s + k * 6, y - k * 34 * s), (x + 34 * s - k * 6, y - k * 34 * s), (x, y - (k + 1.6) * 34 * s)], fill=(20, 64 + k * 6, 44))
        d.rectangle([x - 4 * s, y, x + 4 * s, y + 16 * s], fill=(60, 40, 30))
    return img


def whiteboard(w=1280, h=960):
    img = Image.new("RGB", (w, h), (232, 234, 232))
    d = ImageDraw.Draw(img)
    for y in range(0, h, 3):
        d.line([(0, y), (w, y)], fill=(228 + (y % 7), 230 + (y % 5), 228))
    d.rectangle([20, 20, w - 20, h - 20], outline=(150, 154, 160), width=10)
    rnd = random.Random(9)
    d.text((70, 60), "Q4 Roadmap", font=font(84, True), fill=(30, 60, 140))
    d.line([(70, 165), (560, 168)], fill=(30, 60, 140), width=6)
    cols = [("NOW", (200, 40, 40)), ("NEXT", (40, 110, 60)), ("LATER", (90, 60, 150))]
    notes = [["Chat PIN lock", "Voice notes", "Thread list"], ["Contacts tab", "Push (ntfy)", "Dark mode"], ["Windows ARM", "iOS TestFlight", "SSO"]]
    palette = [(255, 236, 130), (255, 190, 200), (170, 225, 255), (200, 240, 170)]
    for ci, (title, colr) in enumerate(cols):
        x0 = 70 + ci * 390
        d.text((x0, 210), title, font=font(46, True), fill=colr)
        for ni, text in enumerate(notes[ci]):
            nx, ny = x0 + rnd.randint(-6, 6), 290 + ni * 190 + rnd.randint(-8, 8)
            layer = Image.new("RGBA", (330, 160), (0, 0, 0, 0))
            ld = ImageDraw.Draw(layer)
            ld.rectangle([6, 8, 326, 158], fill=(0, 0, 0, 40))
            ld.rectangle([0, 0, 320, 150], fill=palette[(ci + ni) % 4] + (255,))
            ld.text((22, 52), text, font=font(36, True), fill=(40, 40, 50))
            layer = layer.rotate(rnd.uniform(-4, 4), expand=True, resample=Image.BICUBIC)
            img.paste(layer, (nx, ny), layer)
    d.line([(70, h - 110), (w - 90, h - 100)], fill=(30, 60, 140), width=5)
    for i in range(3):
        x = 90 + i * 380
        d.polygon([(x + 280, h - 118), (x + 310, h - 105), (x + 280, h - 92)], fill=(30, 60, 140))
    return img.filter(ImageFilter.GaussianBlur(0.6))


def floorplan(w=1600, h=1000):
    img = Image.new("RGB", (w, h), (247, 249, 252))
    d = ImageDraw.Draw(img)
    d.rectangle([60, 60, w - 60, h - 60], outline=(33, 55, 91), width=10)
    rooms = [("Open space", 80, 80, 760, 560, (226, 236, 250)), ("Meeting A", 780, 80, 1020, 330, (214, 238, 226)), ("Meeting B", 1020, 80, 1260, 330, (214, 238, 226)),
             ("Kitchen", 1260, 80, 1540, 420, (254, 238, 212)), ("Quiet room", 780, 330, 1020, 560, (236, 226, 248)), ("Phone", 1020, 330, 1140, 560, (236, 226, 248)),
             ("Server", 1140, 330, 1260, 560, (230, 230, 230)), ("Reception", 80, 640, 520, 940, (252, 228, 228)), ("Lounge", 520, 640, 1100, 940, (226, 244, 244)), ("Storage", 1100, 560, 1540, 940, (238, 238, 232))]
    for name, x0, y0, x1, y1, col in rooms:
        d.rectangle([x0, y0, x1, y1], fill=col, outline=(33, 55, 91), width=5)
        d.text(((x0 + x1) // 2, (y0 + y1) // 2), name, font=font(34, True), fill=(33, 55, 91), anchor="mm")
    for r in range(3):  # desks
        for c in range(6):
            x, y = 120 + c * 105, 150 + r * 135
            d.rectangle([x, y, x + 80, y + 46], fill=(255, 255, 255), outline=(120, 135, 160), width=3)
            d.ellipse([x + 28, y + 54, x + 52, y + 78], fill=(33, 55, 91))
    d.text((90, h - 52), "Floor 3 - Perth HQ (not to scale)", font=font(26), fill=(90, 100, 120), anchor="lm")
    return img


def dashboard(w=1920, h=1080):
    img = Image.new("RGB", (w, h), (244, 246, 250))
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, 260, h], fill=(33, 55, 91))
    d.text((40, 50), "WorkChat", font=font(40, True), fill="white")
    for i, t in enumerate(["Overview", "Messages", "Channels", "Files", "People", "Settings"]):
        y = 160 + i * 70
        if i == 0:
            d.rounded_rectangle([20, y - 14, 240, y + 44], radius=14, fill=(30, 91, 184))
        d.text((50, y), t, font=font(28), fill="white")
    d.text((320, 50), "Workspace analytics - last 30 days", font=font(40, True), fill=(25, 39, 63))
    kpis = [("Active users", "9,482", "+4.2%"), ("Messages / day", "58.1k", "+11%"), ("Files shared", "3,207", "+2.9%"), ("Avg reply time", "3m 12s", "-18%")]
    for i, (t, v, c) in enumerate(kpis):
        x = 320 + i * 390
        d.rounded_rectangle([x, 140, x + 360, 310], radius=22, fill="white", outline=(222, 228, 238), width=2)
        d.text((x + 28, 164), t, font=font(24), fill=(110, 120, 140))
        d.text((x + 28, 206), v, font=font(52, True), fill=(25, 39, 63))
        d.text((x + 28, 268), c, font=font(24, True), fill=(20, 150, 100) if c[0] == "+" or t.startswith("Avg") else (200, 60, 60))
    d.rounded_rectangle([320, 350, 1160, 1020], radius=22, fill="white", outline=(222, 228, 238), width=2)
    d.text((350, 376), "Messages by day", font=font(28, True), fill=(25, 39, 63))
    rnd = random.Random(4)
    for i in range(30):
        bh = 120 + int(rnd.random() * 380) + (i % 7 in (5, 6)) * -120
        x = 360 + i * 26
        d.rounded_rectangle([x, 960 - bh, x + 16, 960], radius=5, fill=(30, 91, 184) if i < 29 else (33, 55, 91))
    d.rounded_rectangle([1200, 350, 1860, 1020], radius=22, fill="white", outline=(222, 228, 238), width=2)
    d.text((1230, 376), "Top channels", font=font(28, True), fill=(25, 39, 63))
    for i, (n, p) in enumerate([("#engineering-deployments", 0.92), ("#company-announcements", 0.74), ("#product-strategy", 0.63), ("#design-system", 0.41), ("#random-lunch", 0.33)]):
        y = 450 + i * 105
        d.text((1230, y), n, font=font(26), fill=(25, 39, 63))
        d.rounded_rectangle([1230, y + 44, 1830, y + 64], radius=10, fill=(230, 235, 244))
        d.rounded_rectangle([1230, y + 44, 1230 + int(600 * p), y + 64], radius=10, fill=(30, 91, 184))
    return img


def moodboard(w=1200, h=1200):
    img = Image.new("RGB", (w, h), (255, 255, 255))
    d = ImageDraw.Draw(img)
    sw = [("Navy Classic", "#21375B"), ("Deep Navy", "#19273F"), ("Link Blue", "#1E5BB8"), ("Slate", "#4A5F86"), ("Page", "#F4F6FA"), ("Mint", "#19C37D"), ("Amber", "#F5A524"), ("Rose", "#E5484D"), ("Sky", "#D6E4FF")]
    n = 0
    for r in range(3):
        for c in range(3):
            x0, y0 = c * 400, r * 400
            col = sw[n][1]
            d.rectangle([x0, y0, x0 + 400, y0 + 400], fill=col)
            lum = sum(int(col[i:i + 2], 16) for i in (1, 3, 5)) / 3
            d.text((x0 + 30, y0 + 330), sw[n][0], font=font(34, True), fill="white" if lum < 140 else (25, 39, 63))
            d.text((x0 + 30, y0 + 364), col, font=font(24), fill="white" if lum < 140 else (60, 70, 90))
            n += 1
    return img


def product_mockup(w=900, h=1600):
    img = vgradient(w, h, [(0, (214, 228, 255)), (1, (244, 246, 250))])
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([100, 100, 800, 1500], radius=80, fill=(20, 24, 34))
    d.rounded_rectangle([118, 118, 782, 1482], radius=64, fill=(244, 246, 250))
    d.rectangle([118, 118, 782, 300], fill=(255, 255, 255))
    d.text((160, 170), "Chats", font=font(54, True), fill=(25, 39, 63))
    d.text((160, 238), "Online", font=font(26), fill=(30, 91, 184))
    rnd = random.Random(2)
    cols = [(33, 55, 91), (120, 150, 190), (90, 160, 140), (200, 120, 90), (140, 110, 180), (60, 120, 200)]
    names = ["Executive Leadership", "Product Strategy", "Elena Vance", "David Kim", "Marcus Brody", "Priya Patel"]
    for i, n in enumerate(names):
        y = 330 + i * 165
        d.ellipse([150, y, 250, y + 100], fill=cols[i])
        d.text((276, y + 8), n, font=font(34, True), fill=(25, 39, 63))
        d.rounded_rectangle([276, y + 62, 276 + rnd.randint(260, 400), y + 78], radius=8, fill=(214, 220, 232))
        if i % 2 == 0:
            d.ellipse([700, y + 30, 740, y + 70], fill=(33, 55, 91))
    d.ellipse([650, 1340, 750, 1440], fill=(33, 55, 91))
    return img


def skyline(w=1920, h=1080):
    img = vgradient(w, h, [(0, (8, 12, 38)), (0.6, (36, 40, 96)), (1, (120, 80, 130))])
    d = ImageDraw.Draw(img)
    rnd = random.Random(21)
    for _ in range(220):
        d.point((rnd.randint(0, w), rnd.randint(0, int(h * 0.5))), fill=(220 + rnd.randint(0, 35),) * 3)
    d.ellipse([1500, 140, 1620, 260], fill=(250, 246, 220))
    x = 0
    for layer, (col, hmin, hmax) in enumerate([((26, 28, 64), 260, 560), ((14, 16, 40), 180, 460)]):
        x = -20
        while x < w:
            bw = rnd.randint(70, 170)
            bh = rnd.randint(hmin, hmax)
            top = h - bh - (40 if layer == 0 else 0)
            d.rectangle([x, top, x + bw, h], fill=col)
            for wy in range(top + 20, h - 30, 34):
                for wx in range(x + 12, x + bw - 18, 26):
                    if rnd.random() < 0.38:
                        d.rectangle([wx, wy, wx + 10, wy + 16], fill=(255, 214 + rnd.randint(0, 30), 120))
            x += bw + rnd.randint(0, 14)
    return img.filter(ImageFilter.GaussianBlur(0.8))


def receipt(w=800, h=1100):
    img = Image.new("RGB", (w, h), (236, 232, 224))
    d = ImageDraw.Draw(img)
    d.rectangle([110, 30, 690, 1070], fill=(252, 252, 250))
    m = mono(30)
    lines = ["BLUE HERON CAFE", "14 Mill Point Rd", "South Perth WA", "------------------------", "Table 6            Fri 14 Nov", "------------------------", "2x Flat white          9.00", "1x Chicken wrap       16.50", "1x Avocado toast      18.00", "1x Sparkling water     5.50", "2x Almond croissant   13.00", "------------------------", "SUBTOTAL              62.00", "GST incl.              5.64", "TOTAL AUD             62.00", "------------------------", "VISA ****4417  APPROVED", "Team lunch - Product Ops", "", "Thank you!"]
    for i, t in enumerate(lines):
        d.text((150, 80 + i * 44), t, font=m, fill=(40, 40, 40))
    return img.rotate(-1.6, expand=False, fillcolor=(236, 232, 224), resample=Image.BICUBIC)


def bokeh(w=1200, h=1200):
    img = vgradient(w, h, [(0, (60, 30, 40)), (0.5, (150, 80, 60)), (1, (250, 190, 110))])
    layer = Image.new("RGB", (w, h), (0, 0, 0))
    ld = ImageDraw.Draw(layer)
    rnd = random.Random(8)
    for _ in range(46):
        r = rnd.randint(40, 150)
        x, y = rnd.randint(0, w), rnd.randint(0, h)
        c = rnd.choice([(255, 220, 150), (255, 170, 110), (255, 240, 200), (230, 130, 100)])
        ld.ellipse([x - r, y - r, x + r, y + r], fill=tuple(int(v * rnd.uniform(0.2, 0.55)) for v in c))
    layer = layer.filter(ImageFilter.GaussianBlur(7))
    from PIL import ImageChops
    img = ImageChops.add(img, layer)
    d = ImageDraw.Draw(img)
    cup_x, cup_y = 600, 760  # a coffee cup in the foreground
    d.ellipse([cup_x - 210, cup_y + 150, cup_x + 210, cup_y + 250], fill=(50, 30, 30))
    d.polygon([(cup_x - 170, cup_y - 90), (cup_x + 170, cup_y - 90), (cup_x + 120, cup_y + 190), (cup_x - 120, cup_y + 190)], fill=(246, 244, 240))
    d.ellipse([cup_x - 170, cup_y - 120, cup_x + 170, cup_y - 60], fill=(110, 66, 40))
    d.arc([cup_x + 130, cup_y - 40, cup_x + 260, cup_y + 100], -80, 90, fill=(246, 244, 240), width=24)
    d.text((cup_x, 190), "Friday lunch", font=font(86, True), fill=(255, 244, 226), anchor="mm")
    return img.filter(ImageFilter.GaussianBlur(0.7))


PHOTOS = {
    "harbour-sunset.jpg": harbour_sunset,
    "team-offsite-lake.jpg": offsite_lake,
    "whiteboard-roadmap.jpg": whiteboard,
    "office-floorplan.png": floorplan,
    "dashboard-metrics.png": dashboard,
    "brand-moodboard.jpg": moodboard,
    "product-mockup.jpg": product_mockup,
    "city-skyline-night.jpg": skyline,
    "receipt-scan.png": receipt,
    "friday-lunch.jpg": bokeh,
}


def save_photos(outdir):
    for name, fn in PHOTOS.items():
        img = fn().convert("RGB")
        path = outdir / name
        if name.endswith(".png"):
            img.save(path, optimize=True)
        else:
            img.save(path, quality=84, optimize=True, progressive=True)


# ------------------------------------------------------------------ people and channel pictures

def person_avatar(path, skin, hair, style, shirt, bg, glasses=False, beard=False, size=512):
    S = 1024
    img = vgradient(S, S, [(0, bg[0]), (1, bg[1])])
    d = ImageDraw.Draw(img)
    cx = S // 2
    d.ellipse([cx - 330, 760, cx + 330, 1400], fill=shirt)  # shoulders
    d.polygon([(cx - 90, 770), (cx + 90, 770), (cx, 880)], fill=skin)  # collar
    d.rectangle([cx - 70, 640, cx + 70, 800], fill=tuple(int(v * 0.92) for v in skin))  # neck
    hair_back = style in ("long", "bob", "curly")
    if hair_back:
        r = {"long": (250, 360, 600), "bob": (230, 310, 560), "curly": (270, 270, 470)}[style]
        d.rounded_rectangle([cx - r[0], 250, cx + r[0], 250 + r[2]], radius=190, fill=hair)
    if style == "bun":
        d.ellipse([cx - 90, 120, cx + 90, 300], fill=hair)
    d.ellipse([cx - 190, 300, cx + 190, 740], fill=skin)  # head
    d.ellipse([cx - 215, 505, cx - 170, 585], fill=skin)
    d.ellipse([cx + 170, 505, cx + 215, 585], fill=skin)
    if style in ("short", "buzz", "bun"):
        top = 270 if style != "buzz" else 300
        d.pieslice([cx - 205, top, cx + 205, top + 400], 180, 360, fill=hair)
        if style == "short":
            d.polygon([(cx - 200, 460), (cx - 120, 360), (cx + 20, 330), (cx - 60, 400), (cx - 170, 520)], fill=hair)
    elif style in ("long", "bob", "curly"):
        d.pieslice([cx - 210, 270, cx + 210, 700], 180, 360, fill=hair)
        d.polygon([(cx - 210, 480), (cx - 40, 330), (cx + 70, 300), (cx - 90, 420), (cx - 190, 560)], fill=hair)
    if style == "bald":
        pass
    if beard:
        d.pieslice([cx - 185, 450, cx + 185, 790], 10, 170, fill=hair)
        d.ellipse([cx - 70, 600, cx + 70, 640], fill=skin)
    for ex in (-72, 72):
        d.ellipse([cx + ex - 17, 500, cx + ex + 17, 536], fill=(40, 30, 30))
        d.arc([cx + ex - 38, 460, cx + ex + 38, 495], 200, 340, fill=(60, 40, 30), width=9)
    d.arc([cx - 62, 570, cx + 62, 680], 25, 155, fill=(120, 50, 50) if not beard else (80, 30, 30), width=12)
    if glasses:
        for ex in (-72, 72):
            d.ellipse([cx + ex - 52, 478, cx + ex + 52, 560], outline=(30, 30, 40), width=9)
        d.line([(cx - 20, 515), (cx + 20, 515)], fill=(30, 30, 40), width=8)
    img = img.resize((size, size), Image.LANCZOS)
    img.save(path, "PNG", optimize=True)


def icon_tile(path, kind, size=512, colors=((44, 70, 112), (25, 39, 63))):
    S = 1024
    img = vgradient(S, S, [(0, colors[0]), (1, colors[1])])
    d = ImageDraw.Draw(img)
    W = (255, 255, 255)
    cx = cy = S // 2
    if kind == "megaphone":
        d.polygon([(300, 440), (560, 440), (780, 300), (780, 724), (560, 584), (300, 584)], fill=W)
        d.rounded_rectangle([250, 440, 340, 584], radius=30, fill=W)
        d.polygon([(340, 584), (440, 584), (470, 760), (380, 760)], fill=W)
        d.arc([760, 400, 880, 624], -60, 60, fill=W, width=26)
    elif kind == "terminal":
        d.rounded_rectangle([220, 280, 804, 744], radius=60, outline=W, width=36)
        d.line([(320, 440), (420, 512), (320, 584)], fill=W, width=38, joint="curve")
        d.line([(480, 600), (640, 600)], fill=W, width=38)
    elif kind == "chart":
        pts = [(230, 700), (400, 520), (540, 610), (800, 310)]
        d.line(pts, fill=W, width=44, joint="curve")
        for p in pts:
            d.ellipse([p[0] - 38, p[1] - 38, p[0] + 38, p[1] + 38], fill=W)
    elif kind == "palette":
        d.ellipse([240, 260, 784, 764], fill=W)
        d.ellipse([560, 560, 700, 700], fill=colors[1])
        for (x, y, c) in ((350, 400, (229, 72, 77)), (480, 340, (245, 165, 36)), (610, 400, (25, 195, 125)), (330, 540, (30, 91, 184))):
            d.ellipse([x - 42, y - 42, x + 42, y + 42], fill=c)
    elif kind == "star":
        pts = []
        for i in range(10):
            a = -math.pi / 2 + i * math.pi / 5
            r = 320 if i % 2 == 0 else 135
            pts.append((cx + r * math.cos(a), cy + 20 + r * math.sin(a)))
        d.polygon(pts, fill=W)
    elif kind == "funnel":
        d.polygon([(230, 270), (794, 270), (580, 560), (580, 780), (444, 700), (444, 560)], fill=W)
    elif kind == "heart":
        d.ellipse([250, 290, 520, 560], fill=W)
        d.ellipse([504, 290, 774, 560], fill=W)
        d.polygon([(262, 470), (762, 470), (512, 780)], fill=W)
    elif kind == "scale":
        d.line([(512, 270), (512, 760)], fill=W, width=36)
        d.line([(300, 340), (724, 340)], fill=W, width=36)
        d.rectangle([380, 750, 644, 790], fill=W)
        for x in (300, 724):
            d.line([(x, 340), (x - 90, 560)], fill=W, width=18)
            d.line([(x, 340), (x + 90, 560)], fill=W, width=18)
            d.pieslice([x - 130, 500, x + 130, 640], 0, 180, fill=W)
    elif kind == "alert":
        d.polygon([(512, 250), (810, 770), (214, 770)], fill=W)
        d.rounded_rectangle([488, 430, 536, 640], radius=24, fill=colors[1])
        d.ellipse([486, 670, 538, 722], fill=colors[1])
    elif kind == "coffee":
        d.polygon([(280, 400), (720, 400), (670, 720), (330, 720)], fill=W)
        d.arc([650, 430, 840, 650], -80, 90, fill=W, width=40)
        for x in (380, 500, 620):
            d.arc([x - 40, 250, x + 40, 380], 270, 90, fill=W, width=22)
    elif kind == "rocket":
        d.polygon([(512, 240), (640, 420), (640, 640), (384, 640), (384, 420)], fill=W)
        d.ellipse([462, 400, 562, 500], fill=colors[1])
        d.polygon([(384, 560), (290, 700), (384, 660)], fill=W)
        d.polygon([(640, 560), (734, 700), (640, 660)], fill=W)
        d.polygon([(450, 650), (574, 650), (512, 800)], fill=(245, 165, 36))
    elif kind == "calendar":
        d.rounded_rectangle([250, 290, 774, 760], radius=50, fill=W)
        d.rectangle([250, 290, 774, 410], fill=(229, 72, 77))
        for r in range(3):
            for c in range(4):
                d.rounded_rectangle([300 + c * 118, 450 + r * 100, 370 + c * 118, 510 + r * 100], radius=12, fill=colors[0])
    img = img.resize((size, size), Image.LANCZOS)
    img.save(path, "PNG", optimize=True)
