#!/usr/bin/env python3
"""
TI-84 Plus CE Button Extraction Tool

Extracts individual button images (PNG crops) from a calculator photo.

Usage:
    python extract_buttons.py --map           # Interactive mode: click to define button regions
    python extract_buttons.py --extract       # Crop buttons from button_regions.json
    python extract_buttons.py --preview       # Show overlay of current regions on source image
"""

import argparse
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

SCRIPT_DIR = Path(__file__).parent
PROJECT_DIR = SCRIPT_DIR.parent
SOURCE_IMAGE = PROJECT_DIR / "hires-ti84ce-cropped.png"
REGIONS_FILE = SCRIPT_DIR / "button_regions.json"
ASSETS_DIR = PROJECT_DIR / "assets"
BUTTONS_DIR = ASSETS_DIR / "buttons"


def sanitize_name(name):
    """Convert button name to a safe filename."""
    # Full-name matches (checked first before character-level replacements)
    full_name_map = {
        "X,T,θ,n": "xttn", "x⁻¹": "x_inv", "x²": "x_sq",
        "(−)": "neg", "sto→": "sto", "y=": "y_eq",
    }
    if name in full_name_map:
        return f"btn_{full_name_map[name]}"
    # Character-level replacements
    char_replacements = {
        "÷": "div", "×": "mul", "−": "sub", "+": "add",
        "^": "pow", "(": "lparen", ")": "rparen",
        ",": "comma", ".": "dot", "θ": "theta",
        " ": "_", "/": "_",
    }
    result = name
    for old, new in char_replacements.items():
        result = result.replace(old, new)
    result = "".join(c if c.isalnum() or c == "_" else "" for c in result)
    return f"btn_{result.lower()}"


def sample_body_color(image, buttons, offset_x, offset_y):
    """Sample average color from body area between buttons."""
    w, h = image.size
    # Collect pixels from thin strips between button columns (left margin area)
    pixels = []
    for y in range(h // 4, h * 3 // 4):
        for x in range(2, min(30, w)):
            pixels.append(image.getpixel((x, y)))
    if not pixels:
        return (30, 30, 30)
    r = sum(p[0] for p in pixels) // len(pixels)
    g = sum(p[1] for p in pixels) // len(pixels)
    b = sum(p[2] for p in pixels) // len(pixels)
    return (r, g, b)


def blank_buttons(image, buttons, offset_x, offset_y):
    """Fill button face areas with body-matching dark grey.

    Removes the original button faces so only the overlay button images show.
    Secondary text labels (printed on the body between buttons) are preserved.
    """
    fill_color = (22, 19, 20)
    draw = ImageDraw.Draw(image)
    for btn in buttons:
        name = btn.get("name", "")
        if name == "dpad":
            continue
        bx = btn["x"] - offset_x
        extra = {"9": 4, "sub": 4, "mul": 2, "div": 2, "lparen": 2, "rparen": 2, "enter": 5, "log": 2, "7": 2}
        extra_down = {"3": 3}
        expand = extra.get(name, 1)
        expand_dn = extra_down.get(name, 0)
        by = btn["y"] - offset_y - expand
        draw.rectangle([bx, by, bx + btn["w"], by + expand + btn["h"] + expand_dn], fill=fill_color)


def cmd_extract(args):
    """Crop buttons from source image and save as PNGs."""
    if not REGIONS_FILE.exists():
        print(f"Error: {REGIONS_FILE} not found. Run --map first.")
        sys.exit(1)

    with open(REGIONS_FILE) as f:
        regions = json.load(f)

    img = Image.open(SOURCE_IMAGE)
    print(f"Source image: {img.size[0]}x{img.size[1]}")

    # Subtle sharpening: UnsharpMask(radius=1, percent=120, threshold=2)
    sharpen = ImageFilter.UnsharpMask(radius=1, percent=120, threshold=2)

    ASSETS_DIR.mkdir(exist_ok=True)
    BUTTONS_DIR.mkdir(exist_ok=True)

    manifest = {
        "source": str(SOURCE_IMAGE.name),
        "keypad_bounds": regions["keypad_bounds"],
        "buttons": [],
    }

    for btn in regions["buttons"]:
        name = btn["name"]
        safe_name = sanitize_name(name)

        # Crop button from source image and sharpen
        crop = img.crop((btn["x"], btn["y"], btn["x"] + btn["w"], btn["y"] + btn["h"]))
        crop = crop.filter(sharpen)
        png_path = BUTTONS_DIR / f"{safe_name}.png"
        crop.save(png_path)

        manifest["buttons"].append({
            "name": name,
            "label": btn.get("label", name),
            "row": btn.get("row"),
            "col": btn.get("col"),
            "style": btn.get("style", "dark"),
            "img": f"buttons/{safe_name}.png",
            "x": btn["x"], "y": btn["y"],
            "w": btn["w"], "h": btn["h"],
        })

        print(f"  {safe_name}: {btn['w']}x{btn['h']}")

    # Crop keypad body background
    kp = regions["keypad_bounds"]
    body_crop = img.crop((kp["x"], kp["y"], kp["x"] + kp["w"], kp["y"] + kp["h"]))
    body_crop = body_crop.filter(sharpen)
    blank_buttons(body_crop, regions["buttons"], kp["x"], kp["y"])
    body_path = ASSETS_DIR / "keypad_body.png"
    body_crop.save(body_path)
    print(f"\nKeypad body: {body_path}")

    # Crop screen branding strip (TI-84 Plus CE text)
    if "screen_branding" in regions:
        br = regions["screen_branding"]
        branding_crop = img.crop((br["x"], br["y"], br["x"] + br["w"], br["y"] + br["h"]))
        branding_crop = branding_crop.filter(sharpen)
        branding_path = ASSETS_DIR / "screen_branding.png"
        branding_crop.save(branding_path)
        print(f"Screen branding: {branding_path} ({br['w']}x{br['h']})")

    # Crop screen bezel (with LCD area blacked out)
    if "screen_bezel" in regions:
        sb = regions["screen_bezel"]
        bezel_crop = img.crop((sb["x"], sb["y"], sb["x"] + sb["w"], sb["y"] + sb["h"]))
        bezel_crop = bezel_crop.filter(sharpen)

        # Black out the LCD opening so the photo's screen content doesn't show through
        if "lcd_opening" in regions:
            lcd = regions["lcd_opening"]
            bezel_draw = ImageDraw.Draw(bezel_crop)
            # LCD coords relative to bezel crop
            lx = lcd["x"] - sb["x"]
            ly = lcd["y"] - sb["y"]
            bezel_draw.rectangle([lx, ly, lx + lcd["w"], ly + lcd["h"]], fill=(0, 0, 0))

        bezel_path = ASSETS_DIR / "screen_bezel.png"
        bezel_crop.save(bezel_path)
        print(f"Screen bezel: {bezel_path} ({sb['w']}x{sb['h']})")

    # Crop combined calculator body (bezel + keypad, with LCD blacked out)
    if "calculator_body" in regions:
        cb = regions["calculator_body"]
        body_combined = img.crop((cb["x"], cb["y"], cb["x"] + cb["w"], cb["y"] + cb["h"]))
        body_combined = body_combined.filter(sharpen)

        # Black out the LCD opening
        if "lcd_opening" in regions:
            lcd = regions["lcd_opening"]
            draw = ImageDraw.Draw(body_combined)
            lx = lcd["x"] - cb["x"]
            ly = lcd["y"] - cb["y"]
            draw.rectangle([lx, ly, lx + lcd["w"], ly + lcd["h"]], fill=(0, 0, 0))

        # Black out button faces so press-travel animation doesn't show originals
        blank_buttons(body_combined, regions["buttons"], cb["x"], cb["y"])

        combined_path = ASSETS_DIR / "calculator_body.png"
        body_combined.save(combined_path)
        print(f"Calculator body: {combined_path} ({cb['w']}x{cb['h']})")

    # Save manifest
    manifest_path = ASSETS_DIR / "button_manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)
    print(f"Manifest: {manifest_path}")

    # Also copy to web/public/buttons/ if it exists
    import shutil
    web_buttons = PROJECT_DIR / "web" / "public" / "buttons"
    if web_buttons.parent.exists():
        web_buttons.mkdir(exist_ok=True)
        for png in BUTTONS_DIR.glob("*.png"):
            shutil.copy2(png, web_buttons / png.name)
        shutil.copy2(body_path, web_buttons / "keypad_body.png")
        if "screen_bezel" in regions:
            shutil.copy2(bezel_path, web_buttons / "screen_bezel.png")
        if "screen_branding" in regions:
            shutil.copy2(branding_path, web_buttons / "screen_branding.png")
        if "calculator_body" in regions:
            shutil.copy2(combined_path, web_buttons / "calculator_body.png")
        print(f"Copied to {web_buttons}")

    print(f"\nGenerated {len(regions['buttons'])} button PNGs in {BUTTONS_DIR}")


def cmd_preview(args):
    """Show overlay of current regions on source image."""
    if not REGIONS_FILE.exists():
        print(f"Error: {REGIONS_FILE} not found.")
        sys.exit(1)

    with open(REGIONS_FILE) as f:
        regions = json.load(f)

    img = Image.open(SOURCE_IMAGE).copy()
    draw = ImageDraw.Draw(img, "RGBA")

    # Draw screen bezel bounds
    if "screen_bezel" in regions:
        sb = regions["screen_bezel"]
        draw.rectangle(
            [sb["x"], sb["y"], sb["x"] + sb["w"], sb["y"] + sb["h"]],
            outline=(0, 200, 255, 200), width=3
        )
        draw.text((sb["x"] + 6, sb["y"] + 6), "screen_bezel", fill=(0, 200, 255, 230))

    # Draw LCD opening
    if "lcd_opening" in regions:
        lcd = regions["lcd_opening"]
        draw.rectangle(
            [lcd["x"], lcd["y"], lcd["x"] + lcd["w"], lcd["y"] + lcd["h"]],
            outline=(255, 100, 100, 220), width=2
        )
        draw.text((lcd["x"] + 6, lcd["y"] + 6), "lcd_opening", fill=(255, 100, 100, 230))

    # Draw calculator body bounds
    if "calculator_body" in regions:
        cb = regions["calculator_body"]
        draw.rectangle(
            [cb["x"], cb["y"], cb["x"] + cb["w"], cb["y"] + cb["h"]],
            outline=(255, 165, 0, 200), width=3
        )
        draw.text((cb["x"] + 6, cb["y"] + 6), "calculator_body", fill=(255, 165, 0, 230))

    # Draw keypad bounds
    kp = regions["keypad_bounds"]
    draw.rectangle(
        [kp["x"], kp["y"], kp["x"] + kp["w"], kp["y"] + kp["h"]],
        outline=(0, 255, 0, 180), width=3
    )

    style_overlay = {
        "dark": (100, 100, 100, 100),
        "yellow": (106, 182, 230, 100),
        "green": (109, 190, 69, 100),
        "white": (230, 230, 230, 100),
        "blue": (220, 220, 220, 100),
        "arrow": (74, 74, 74, 100),
    }

    for btn in regions["buttons"]:
        x, y, w, h = btn["x"], btn["y"], btn["w"], btn["h"]
        style = btn.get("style", "dark")
        color = style_overlay.get(style, (100, 100, 100, 100))
        draw.rectangle([x, y, x + w, y + h], fill=color, outline=(255, 255, 0, 200), width=1)
        label = btn.get("label", btn["name"])
        draw.text((x + 4, y + 2), label, fill=(255, 255, 255, 230))

    preview_path = ASSETS_DIR / "preview_overlay.png"
    ASSETS_DIR.mkdir(exist_ok=True)
    img.save(preview_path)
    print(f"Preview saved to: {preview_path}")


def cmd_map(args):
    """Interactive mode to define button regions by clicking."""
    try:
        import matplotlib
        matplotlib.use("TkAgg")
        import matplotlib.pyplot as plt
        import matplotlib.patches as patches
    except ImportError:
        print("Error: matplotlib required. Install with: pip3 install matplotlib")
        sys.exit(1)

    img = Image.open(SOURCE_IMAGE)

    existing = {}
    if REGIONS_FILE.exists():
        with open(REGIONS_FILE) as f:
            data = json.load(f)
            for btn in data.get("buttons", []):
                existing[btn["name"]] = btn

    print("\n=== Interactive Button Region Mapper ===")
    print("  Click TOP-LEFT then BOTTOM-RIGHT of each button.")
    print("  Press 'q' to save and quit, 'd' to delete last.")

    fig, ax = plt.subplots(1, 1, figsize=(10, 10))
    ax.imshow(img)
    ax.set_title("Click corners of each button. 'q' to save.")

    for name, btn in existing.items():
        rect = patches.Rectangle(
            (btn["x"], btn["y"]), btn["w"], btn["h"],
            linewidth=1, edgecolor='yellow', facecolor='none', alpha=0.5
        )
        ax.add_patch(rect)
        ax.text(btn["x"] + 2, btn["y"] + 12, name, color='yellow', fontsize=7)

    clicks = []
    new_buttons = list(existing.values())

    def on_click(event):
        if event.inaxes != ax:
            return
        clicks.append((int(event.xdata), int(event.ydata)))
        if len(clicks) % 2 == 1:
            ax.plot(event.xdata, event.ydata, 'r+', markersize=10)
            fig.canvas.draw()
        else:
            x1, y1 = clicks[-2]
            x2, y2 = clicks[-1]
            x, y = min(x1, x2), min(y1, y2)
            w, h = abs(x2 - x1), abs(y2 - y1)
            rect = patches.Rectangle((x, y), w, h, linewidth=2, edgecolor='lime', facecolor='none')
            ax.add_patch(rect)
            fig.canvas.draw()

            name = input(f"  Button name ({x},{y} {w}x{h}): ").strip()
            if name:
                style = input(f"  Style [dark]: ").strip() or "dark"
                row = input(f"  Row: ").strip()
                col = input(f"  Col: ").strip()
                label = input(f"  Label [{name}]: ").strip() or name
                btn = {"name": name, "label": label, "row": int(row) if row else 0,
                       "col": int(col) if col else 0, "style": style,
                       "x": x, "y": y, "w": w, "h": h}
                new_buttons.append(btn)
                ax.text(x + 2, y + 12, name, color='lime', fontsize=8)
                fig.canvas.draw()

    def on_key(event):
        if event.key == 'q':
            plt.close()
        elif event.key == 'd' and new_buttons:
            removed = new_buttons.pop()
            print(f"  Removed: {removed['name']}")

    fig.canvas.mpl_connect('button_press_event', on_click)
    fig.canvas.mpl_connect('key_press_event', on_key)
    plt.tight_layout()
    plt.show()

    if new_buttons:
        all_x = [b["x"] for b in new_buttons]
        all_y = [b["y"] for b in new_buttons]
        all_r = [b["x"] + b["w"] for b in new_buttons]
        all_b = [b["y"] + b["h"] for b in new_buttons]
        result = {
            "source_image": str(SOURCE_IMAGE.name),
            "image_size": list(img.size),
            "keypad_bounds": {
                "x": min(all_x) - 20, "y": min(all_y) - 20,
                "w": max(all_r) - min(all_x) + 40, "h": max(all_b) - min(all_y) + 40,
            },
            "buttons": new_buttons,
        }
        with open(REGIONS_FILE, "w") as f:
            json.dump(result, f, indent=2)
        print(f"\nSaved {len(new_buttons)} regions to {REGIONS_FILE}")


def main():
    parser = argparse.ArgumentParser(description="TI-84 Plus CE Button Extraction Tool")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--map", action="store_true", help="Interactive region mapping")
    group.add_argument("--extract", action="store_true", help="Crop buttons as PNGs")
    group.add_argument("--preview", action="store_true", help="Preview overlay")

    args = parser.parse_args()
    if not SOURCE_IMAGE.exists():
        print(f"Error: {SOURCE_IMAGE} not found")
        sys.exit(1)

    if args.map:
        cmd_map(args)
    elif args.extract:
        cmd_extract(args)
    elif args.preview:
        cmd_preview(args)


if __name__ == "__main__":
    main()
