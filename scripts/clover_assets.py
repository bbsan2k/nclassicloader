"""Decode shipped Reed font containers and retain Clover's real visual assets.

No target code is run. Container format observed in this firmware:
u8 version, u32le BMFont-byte-count, u32le PNG-byte-count, BMFont v3, PNG.
"""
import hashlib
import json
import pathlib
import shutil
import struct


def decode_font(data):
    if len(data) < 9 or data[0] != 1:
        raise ValueError("Unsupported Reed font container")
    metrics_size, image_size = struct.unpack_from("<II", data, 1)
    if 9 + metrics_size + image_size != len(data):
        raise ValueError("Font section lengths do not match container")
    metrics = data[9:9 + metrics_size]
    image = data[9 + metrics_size:]
    if not metrics.startswith(b"BMF\x03") or not image.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("Invalid BMFont/PNG sections")
    result = {"glyphs": {}, "kernings": {}}
    pos = 4
    while pos < len(metrics):
        if pos + 5 > len(metrics):
            raise ValueError("Truncated BMFont block")
        kind, size = struct.unpack_from("<BI", metrics, pos)
        pos += 5
        block = metrics[pos:pos + size]
        if len(block) != size:
            raise ValueError("Truncated BMFont payload")
        pos += size
        if kind == 1:
            if size < 2:
                raise ValueError("Truncated BMFont info block")
            result["fontSize"] = abs(struct.unpack_from("<h", block)[0])
        elif kind == 2:
            if size != 15:
                raise ValueError("Unexpected BMFont common block")
            line, base, width, height, pages = struct.unpack_from("<5H", block)
            if pages != 1:
                raise ValueError("Only one-page firmware fonts are supported")
            result.update(lineHeight=line, baseline=base, width=width, height=height)
        elif kind == 4:
            if size % 20:
                raise ValueError("Invalid BMFont character block")
            for offset in range(0, size, 20):
                code, x, y, w, h, dx, dy, advance, page, channel = struct.unpack_from("<I4H3h2B", block, offset)
                if page != 0:
                    raise ValueError("Unexpected glyph page")
                result["glyphs"][str(code)] = dict(x=x, y=y, width=w, height=h, xOffset=dx, yOffset=dy, advance=advance)
        elif kind == 5:
            if size % 10:
                raise ValueError("Invalid BMFont kerning block")
            for offset in range(0, size, 10):
                first, second, amount = struct.unpack_from("<IIh", block, offset)
                result["kernings"][f"{first}:{second}"] = amount
    if "lineHeight" not in result or not result["glyphs"]:
        raise ValueError("Missing font metrics")
    for g in result["glyphs"].values():
        if g["x"] + g["width"] > result["width"] or g["y"] + g["height"] > result["height"]:
            raise ValueError("Glyph outside font atlas")
    return result, image


def prepare_skin(resources, output):
    resources, output = pathlib.Path(resources), pathlib.Path(output)
    output.mkdir(parents=True, exist_ok=True)
    sources = []

    def copy(relative, destination):
        source = resources / relative
        shutil.copy2(source, output / destination)
        sources.append({"path": relative, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})

    copy("sprites/packed.png", "packed.png")
    copy("sprites/packed.json", "packed.json")
    copy("layout/asset/sprite/wallpaper/wall.png", "wall.png")
    copy("animation/cursor_thumbnails.spriteanim", "cursor-thumbnails.json")
    for sound in ["bgm_home", "se_sys_cursor", "se_sys_click"]:
        copy(f"sounds/hvc/{sound}.wav", f"{sound}.wav")
    for name in ["title", "m", "s", "l"]:
        source = resources / f"fonts/{name}.font"
        data = source.read_bytes()
        font, png = decode_font(data)
        (output / f"font-{name}.json").write_text(json.dumps(font, sort_keys=True))
        (output / f"font-{name}.png").write_bytes(png)
        sources.append({"path": f"fonts/{name}.font", "sha256": hashlib.sha256(data).hexdigest()})
    scene = json.loads((resources / "defaultscene.scn").read_text())
    nodes = {}
    world = {}

    def visit(node, path="", parent=(1, 0, 0, 0, 1, 0)):
        path += "/" + node["name"]
        nodes[path] = node
        a, b, c, d, e, f = parent
        A, B, C, D, E, F = node["transform"]
        transform = (a*A+b*D, a*B+b*E, a*C+b*F+c, d*A+e*D, d*B+e*E, d*C+e*F+f)
        world[path] = transform
        for child in node.get("children", []):
            visit(child, path, transform)

    visit(scene["rootWorldNode"])
    home = "/Main/homemenu"
    paths = {
        "title": home + "/gametitle",
        "thumbnails": home + "/thumbnails",
        "hud": home + "/hud",
        "bottomBar": home + "/menubar",
        "topBar": home + "/elements/menubar_upper/menubar",
        "topButtons": home + "/elements/menubar_upper/elements",
    }
    positions = {}
    for key, path in paths.items():
        x, y = world[path][2], world[path][5]
        positions[key] = [640 + x, 360 - y]
    dependencies = json.loads((resources.parent / "dependencies.json").read_text())
    font_names = {r["id"]: pathlib.PurePosixPath(r["path"]).stem for r in dependencies["resources"] if r["type"] == "FontResource"}
    hud_path = home + "/hud/hud_gametitle"
    caption_path = home + "/elements/menubar_upper/elements/sys_menubar_cm5/caption_down"
    label_paths = {"title": home + "/gametitle/gametitle_label", "hud": hud_path + "/hud_item1_menuU/hud_2_label", "tooltip": caption_path + "/Label"}
    labels = {}
    for key, path in label_paths.items():
        label = next(c for c in nodes[path]["components"] if c["type"] == "LabelComponent")
        labels[key] = {"position": [640 + world[path][2], 360 - world[path][5]], "font": label.get("properties", {}).get("fontName", font_names[label["fontResource"]]), "horizontalAnchor": label["horizontalAnchor"], "verticalAnchor": label["verticalAnchor"]}
    caption = next(c for c in nodes[caption_path + "/caption"]["components"] if c["type"] == "SpriteComponent")
    caption_edge = next(c for c in nodes[caption_path + "/caption_r"]["components"] if c["type"] == "SpriteComponent")
    wallpaper = json.loads((resources / "layout/lyt_wallpaper_default.sceneanimation").read_text())
    nodes_by_id = {node["id"]: node for node in nodes.values()}
    cardlist = nodes[home + "/elements/cardlist"]
    marker1 = nodes_by_id[cardlist["properties"]["narabe1"]["linkId"]]
    marker2 = nodes_by_id[cardlist["properties"]["narabe2"]["linkId"]]
    spacing = marker2["transform"][2] - marker1["transform"][2]
    card = json.loads((resources / "prefab/sys_game_card.scn").read_text())["rootWorldNode"]
    inactive = next(n for n in card["children"] if n["name"] == "non_active")
    card_sprite = next(n for n in inactive["children"] if n["name"] == "card")["components"][0]
    layout = {"width": 1280, "height": 720, "positions": positions, "labels": labels,
              "hudItemGap": nodes[hud_path + "/hud_item1_menuU"]["properties"]["VGap"],
              "hudGroupGap": nodes[hud_path]["properties"]["VGap"],
              "tooltip": {"bodyY": 360 - world[caption_path + "/caption"][5], "minimumWidth": caption["size"][0], "height": caption["size"][1], "edgeWidth": caption_edge["size"][0]},
              "cardSize": card_sprite["size"], "cardSpacing": spacing, "wallpaperDuration": wallpaper["duration"]}
    (output / "layout.json").write_text(json.dumps(layout, indent=2))
    for relative in ["defaultscene.scn", "../dependencies.json", "prefab/sys_game_card.scn", "prefab/sys_game_thumbnail.scn", "layout/lyt_wallpaper_default.sceneanimation"]:
        source = resources / relative
        sources.append({"path": relative, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
    (output / "provenance.json").write_text(json.dumps({"sources": sources, "limitations": ["Positions are static scene observations; runtime script mutations are not automatically reproduced.", "Card spacing is the observed separation of the two scene marker nodes."]}, indent=2))
    return layout


if __name__ == "__main__":
    import argparse
    root = pathlib.Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resources", type=pathlib.Path, default=root / "analysis/eur/usr/share/ui/snes-eur/resources")
    parser.add_argument("--output", type=pathlib.Path, default=root / "analysis/clover-skin")
    args = parser.parse_args()
    prepare_skin(args.resources, args.output)
    print(f"Decoded Clover art and fonts into {args.output}")
