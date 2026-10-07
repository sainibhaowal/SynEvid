#!/usr/bin/env python3
"""
Synevid Banner Engine: Patch and render pixel-perfect real information on the sci-fi banner.
Replaces hallucinated AI generator text with real GitHub repository metadata,
maintaining the authentic sci-fi cyberpunk aesthetics, blurred glows, and glassmorphism.
"""

import math
from PIL import Image, ImageDraw, ImageFont, ImageFilter


def draw_star_icon(draw, cx, cy, color):
    r_out, r_in = 6.5, 3.0
    pts = []
    for i in range(10):
        r = r_out if i % 2 == 0 else r_in
        ang = i * math.pi / 5 - math.pi / 2
        pts.append((cx + r * math.cos(ang), cy + r * math.sin(ang)))
    draw.polygon(pts, outline=color, width=1)


def draw_fork_icon(draw, cx, cy, color):
    draw.ellipse([cx - 5, cy - 7, cx - 1, cy - 3], outline=color, width=1)
    draw.ellipse([cx + 1, cy - 7, cx + 5, cy - 3], outline=color, width=1)
    draw.ellipse([cx - 2, cy + 4, cx + 2, cy + 8], outline=color, width=1)
    draw.line([(cx, cy + 1), (cx, cy + 4)], fill=color, width=1)
    draw.line([(cx - 3, cy - 3), (cx - 1, cy + 1)], fill=color, width=1)
    draw.line([(cx + 3, cy - 3), (cx + 1, cy + 1)], fill=color, width=1)


def draw_github_icon(draw, cx, cy, color):
    draw.ellipse([cx - 7, cy - 7, cx + 7, cy + 7], outline=color, width=1)
    draw.ellipse([cx - 5, cy - 4, cx + 5, cy + 5], fill=color)
    draw.polygon([(cx - 5, cy - 5), (cx - 2, cy - 2), (cx - 6, cy - 1)], fill=color)
    draw.polygon([(cx + 5, cy - 5), (cx + 2, cy - 2), (cx + 6, cy - 1)], fill=color)


def patch_banner():
    pristine_path = "/home/ravi/.gemini/antigravity-ide/brain/eefcce9f-85d4-469b-8299-8093a8be4031/synevid_pro_banner_1791308727485.jpg"
    img = Image.open(pristine_path).convert("RGBA")
    w, h = img.size

    font_reg = "/usr/share/fonts/opentype/fira/FiraSans-Regular.otf"
    font_med = "/usr/share/fonts/opentype/fira/FiraSans-Medium.otf"
    font_mono = "/usr/share/fonts/opentype/fira/FiraMono-Regular.otf"

    f_badge = ImageFont.truetype(font_reg, 12)
    f_badge_num = ImageFont.truetype(font_med, 12)
    f_clone_hdr = ImageFont.truetype(font_reg, 13)
    f_clone_url = ImageFont.truetype(font_mono, 9.5)
    f_btn = ImageFont.truetype(font_reg, 12)
    f_hash_hdr = ImageFont.truetype(font_reg, 12)
    f_hash_val = ImageFont.truetype(font_mono, 11)
    f_bot_url = ImageFont.truetype(font_reg, 11)

    # 1. CLEAN TOP-LEFT CLONE UI
    # Strictly limited to x <= 342 so the glowing crystal hexagon at x=358..450 is never touched
    tl_mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(tl_mask).rounded_rectangle([55, 38, 342, 155], radius=10, fill=255)
    bg_blurred = img.filter(ImageFilter.GaussianBlur(15))
    img = Image.composite(bg_blurred, img, tl_mask)

    # 2. CLEAN TOP-RIGHT BADGES
    tr_mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(tr_mask).rectangle([1005, 50, 1360, 102], fill=255)
    img = Image.composite(bg_blurred, img, tr_mask)

    # 3. SELECTIVE PIXEL CLEANING FOR BOTTOM HASH GLYPHS (Zero smudge, preserves circuit traces)
    for x in range(500, 1080):
        for y in range(598, 646):
            r, g, b, _ = img.getpixel((x, y))
            if (g > 120 and b > 130) or (r > 100 and b > 130) or (r + g + b > 300):
                continue
            img.putpixel((x, y), (8, 14, 22, 255))

    # 4. SELECTIVE PIXEL CLEANING FOR BOTTOM URL (Zero smudge, preserves violet circuit trace)
    for x in range(660, 1080):
        for y in range(732, 755):
            r, g, b, _ = img.getpixel((x, y))
            if (r > 90 and b > 130) or (r + g + b > 280):
                continue
            img.putpixel((x, y), (4, 9, 15, 255))

    draw = ImageDraw.Draw(img)

    # 5. DRAW COMPACT TOP-LEFT CLONE UI
    draw.text((68, 45), "Repo clone", font=f_clone_hdr, fill=(185, 210, 230, 230))
    draw.text((145, 45), "git", font=f_clone_hdr, fill=(0, 230, 180, 230))

    # Input Box: [65, 70, 335, 108] (width 270, ends well before crystal hexagon at 358)
    draw.rounded_rectangle([65, 70, 335, 108], radius=6, fill=(16, 24, 34, 230), outline=(45, 68, 95, 200), width=1)
    draw.text((75, 83), "git@github.com:sainibhaowal/SynEvid.git", font=f_clone_url, fill=(185, 210, 230, 255))

    # Divider & Copy Icon
    draw.line([(302, 75), (302, 103)], fill=(35, 52, 75, 180), width=1)
    icx, icy = 314, 83
    draw.rounded_rectangle([icx, icy, icx + 11, icy + 11], radius=2, outline=(120, 150, 180, 200), width=1)
    draw.rounded_rectangle([icx - 3, icy + 3, icx + 8, icy + 14], radius=2, outline=(120, 150, 180, 200), width=1)

    # Action Buttons: width 65 and 57 (ends at x=195)
    draw.rounded_rectangle([65, 120, 130, 150], radius=6, fill=(24, 36, 50, 230), outline=(0, 210, 240, 180), width=1)
    draw.text((82, 127), "Clone", font=f_btn, fill=(0, 230, 255, 255))

    draw.rounded_rectangle([138, 120, 195, 150], radius=6, fill=(16, 24, 34, 180), outline=(35, 52, 75, 150), width=1)
    draw.text((155, 127), "SSH", font=f_btn, fill=(150, 175, 200, 200))

    # 6. DRAW TOP-RIGHT REAL GITHUB BADGES
    bg_col = (20, 28, 38, 235)
    border_col = (48, 68, 92, 220)
    text_col = (215, 230, 242, 255)
    div_col = (40, 58, 80, 220)
    icon_col = (200, 220, 235, 255)

    # Badge 1: sainibhaowal (width 150)
    draw.rounded_rectangle([1010, 56, 1160, 96], radius=6, fill=bg_col, outline=border_col, width=1)
    draw_github_icon(draw, 1028, 76, icon_col)
    draw.text((1042, 68), "sainibhaowal", font=f_badge, fill=text_col)
    draw.polygon([(1146, 75), (1152, 75), (1149, 79)], fill=(140, 165, 190, 220))

    # Badge 2: Star 1 (width 88)
    draw.rounded_rectangle([1168, 56, 1256, 96], radius=6, fill=bg_col, outline=border_col, width=1)
    draw_star_icon(draw, 1184, 76, icon_col)
    draw.text((1196, 68), "Star", font=f_badge, fill=text_col)
    draw.line([(1232, 63), (1232, 89)], fill=div_col, width=1)
    draw.text((1240, 68), "1", font=f_badge_num, fill=text_col)

    # Badge 3: Fork 0 (width 88) - replaces "Fair 0"
    draw.rounded_rectangle([1264, 56, 1352, 96], radius=6, fill=bg_col, outline=border_col, width=1)
    draw_fork_icon(draw, 1280, 76, icon_col)
    draw.text((1292, 68), "Fork", font=f_badge, fill=text_col)
    draw.line([(1328, 63), (1328, 89)], fill=div_col, width=1)
    draw.text((1336, 68), "0", font=f_badge_num, fill=text_col)

    # 7. DRAW BOTTOM HOLOGRAPHIC BLURRED TEXT
    holo_txt = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ht_draw = ImageDraw.Draw(holo_txt)

    # 'hash glyphs' title
    ht_draw.text((515, 602), "hash glyphs", font=f_hash_hdr, fill=(90, 145, 185, 210))

    # Real canonical BLAKE3 digest
    digest_str = "b2d8e4179cf5215c8a8731da8b861e073dc54b05bc118e602"
    ht_draw.text((515, 622), digest_str, font=f_hash_val, fill=(75, 125, 165, 190))

    # Real bottom GitHub URL
    ht_draw.text((680, 738), "https://github.com/sainibhaowal/SynEvid", font=f_bot_url, fill=(80, 125, 165, 190))
    ht_draw.line([(680, 754), (940, 754)], fill=(35, 55, 75, 180), width=1)

    # Holographic bloom blur effect
    glow = holo_txt.filter(ImageFilter.GaussianBlur(radius=0.75))
    holo_combined = Image.alpha_composite(glow, holo_txt)

    # Composite holographic layer
    img = Image.alpha_composite(img, holo_combined)

    # Save final banner
    final = img.convert("RGB")
    final.save("assets/banner.jpg", quality=96)
    print("Master banner saved to assets/banner.jpg successfully!")


if __name__ == "__main__":
    patch_banner()
