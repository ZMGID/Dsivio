"""Embed compressed, cell-anchored pictures in a formatted XLSX.

Standard DrawingML works without rich-value image support. Pictures follow
their cells; object protection prevents accidental dragging in Excel/WPS.
Existing cells remain editable. No password is added.
"""
import argparse
import copy
import hashlib
import json
import os
import posixpath
import re
import tempfile
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from lxml import etree as ET
from PIL import Image

S = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
P = "http://schemas.openxmlformats.org/package/2006/relationships"
C = "http://schemas.openxmlformats.org/package/2006/content-types"
X = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"


def xml(data):
    return ET.fromstring(data, ET.XMLParser(resolve_entities=False, no_network=True))


def serialize(root):
    return ET.tostring(root, encoding="UTF-8", xml_declaration=True, standalone=True)


def sheets(parts):
    rels = {r.get("Id"): r.get("Target") for r in xml(parts["xl/_rels/workbook.xml.rels"])}
    return {s.get("name"): posixpath.normpath(posixpath.join("xl", rels[s.get(f"{{{R}}}id")])).lstrip("/")
            for s in xml(parts["xl/workbook.xml"]).find(f"{{{S}}}sheets")}


def child(parent, namespace, tag, **attrs):
    return ET.SubElement(parent, f"{{{namespace}}}{tag}", **attrs)


def marker(anchor, tag, col, row, offset):
    point = child(anchor, X, tag)
    for name, value in (("col", col), ("colOff", offset), ("row", row), ("rowOff", offset)):
        child(point, X, name).text = str(value)


def embed(source, manifest, output):
    if not manifest:
        raise ValueError("Image manifest is empty")
    with ZipFile(source) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    if "xl/metadata.xml" in parts or any(name.startswith("xl/richData/") for name in parts):
        raise ValueError("Use the report builder's clean XLSX before embedding images")
    paths = sheets(parts)
    roots, entries, seen, media = {}, {}, set(), {}
    for entry in manifest:
        sheet, cell = entry["sheet"], entry["cell"].upper()
        if sheet not in paths or not re.fullmatch(r"[A-Z]+[1-9][0-9]*", cell):
            raise ValueError(f"Invalid picture destination: {sheet}!{cell}")
        if (sheet, cell) in seen:
            raise ValueError(f"Duplicate picture destination: {sheet}!{cell}")
        seen.add((sheet, cell))
        if sheet not in roots:
            roots[sheet] = xml(parts[paths[sheet]])
            if roots[sheet].find(f"{{{S}}}drawing") is not None or roots[sheet].find(f"{{{S}}}sheetProtection") is not None:
                raise ValueError("Generate an unprotected table without existing drawings")
        destination = roots[sheet].find(f".//{{{S}}}c[@r='{cell}']")
        if destination is None or len(destination):
            raise ValueError(f"Picture cell must exist and be empty: {sheet}!{cell}")
        image_path = Path(entry["path"])
        data = image_path.read_bytes()
        with Image.open(image_path) as image:
            if image.size != (320, 320) or image.format != "JPEG" or len(data) > 40 * 1024:
                raise ValueError(f"Run prepare_images.py first: {image_path}")
        digest = hashlib.sha256(data).hexdigest()
        media.setdefault(digest, data)
        entries.setdefault(sheet, []).append((cell, str(entry["productId"]), digest))

    # Clone only the styles used by protected sheets; other sheets stay intact.
    styles_path = "xl/styles.xml"
    if styles_path not in parts:
        raise ValueError("The formatted table must include styles.xml")
    styles = xml(parts[styles_path])
    xfs = styles.find(f"{{{S}}}cellXfs")
    unlocked = {}
    for sheet, root in roots.items():
        for cell in root.iter(f"{{{S}}}c"):
            old = int(cell.get("s", "0"))
            if old not in unlocked:
                clone = copy.deepcopy(xfs[old])
                protection = clone.find(f"{{{S}}}protection")
                if protection is None:
                    protection = child(clone, S, "protection")
                protection.set("locked", "0")
                clone.set("applyProtection", "1")
                unlocked[old] = len(xfs)
                xfs.append(clone)
            cell.set("s", str(unlocked[old]))
    xfs.set("count", str(len(xfs)))
    parts[styles_path] = serialize(styles)

    content = xml(parts["[Content_Types].xml"])
    if not any(c.get("Extension") in ("jpeg", "jpg") for c in content):
        child(content, C, "Default", Extension="jpeg", ContentType="image/jpeg")
    media_paths = {digest: f"xl/media/research-{digest[:20]}.jpeg" for digest in media}
    for digest, data in media.items():
        parts[media_paths[digest]] = data

    for index, (sheet, items) in enumerate(entries.items(), 1):
        drawing_path = f"xl/drawings/research{index}.xml"
        if drawing_path in parts:
            raise ValueError("Research drawing already exists")
        drawing = ET.Element(f"{{{X}}}wsDr", nsmap={"xdr": X, "a": A})
        drawing_rels = ET.Element(f"{{{P}}}Relationships", nsmap={None: P})
        image_rels = {}
        for number, (address, product_id, digest) in enumerate(items, 1):
            letters, digits = re.fullmatch(r"([A-Z]+)([1-9][0-9]*)", address).groups()
            col = 0
            for letter in letters:
                col = col * 26 + ord(letter) - 64
            col -= 1
            row = int(digits) - 1
            # Use fixed square extents within the cell. Reader font metrics
            # differ when converting XLSX character widths to pixels.
            sheet_root = roots[sheet]
            columns = sheet_root.find(f"{{{S}}}cols")
            if columns is None:
                columns = ET.Element(f"{{{S}}}cols")
                sheet_root.insert(list(sheet_root).index(sheet_root.find(f"{{{S}}}sheetData")), columns)
            containing = next((c for c in columns if int(c.get("min")) <= col + 1 <= int(c.get("max"))), None)
            if containing is not None and containing.get("min") != containing.get("max"):
                # Split a range so only the image column's width is changed.
                original_min, original_max = int(containing.get("min")), int(containing.get("max"))
                for start, end in ((original_min, col), (col + 2, original_max)):
                    if start <= end:
                        clone = copy.deepcopy(containing)
                        clone.set("min", str(start))
                        clone.set("max", str(end))
                        columns.append(clone)
                containing.set("min", str(col + 1))
                containing.set("max", str(col + 1))
            if containing is None:
                containing = child(columns, S, "col", min=str(col + 1), max=str(col + 1))
            containing.set("width", "22.142857142857")
            containing.set("customWidth", "1")
            columns[:] = sorted(columns, key=lambda c: int(c.get("min")))
            row_node = sheet_root.find(f"{{{S}}}sheetData/{{{S}}}row[@r='{row + 1}']")
            row_node.set("ht", "120")
            row_node.set("customHeight", "1")
            anchor = child(drawing, X, "twoCellAnchor")
            # Insets keep borders visible; both corners are tied to this cell.
            marker(anchor, "from", col, row, 19050)
            marker(anchor, "to", col, row, 1504950)
            pic = child(anchor, X, "pic")
            nv = child(pic, X, "nvPicPr")
            child(nv, X, "cNvPr", id=str(number), name=f"Product {product_id}", descr=product_id)
            child(child(nv, X, "cNvPicPr"), A, "picLocks", noChangeAspect="1", noMove="1", noResize="1", noSelect="1")
            fill = child(pic, X, "blipFill")
            if digest not in image_rels:
                image_rels[digest] = f"rId{len(image_rels) + 1}"
                child(drawing_rels, P, "Relationship", Id=image_rels[digest], Type=f"{R}/image", Target=posixpath.relpath(media_paths[digest], "xl/drawings"))
            blip = child(fill, A, "blip")
            blip.set(f"{{{R}}}embed", image_rels[digest])
            child(child(fill, A, "stretch"), A, "fillRect")
            shape = child(pic, X, "spPr")
            transform = child(shape, A, "xfrm")
            child(transform, A, "off", x="0", y="0")
            child(transform, A, "ext", cx="1485900", cy="1485900")
            child(child(shape, A, "prstGeom", prst="rect"), A, "avLst")
            child(anchor, X, "clientData", fLocksWithSheet="1")
        parts[drawing_path] = serialize(drawing)
        parts[f"xl/drawings/_rels/research{index}.xml.rels"] = serialize(drawing_rels)
        child(content, C, "Override", PartName=f"/{drawing_path}", ContentType="application/vnd.openxmlformats-officedocument.drawing+xml")
        path = paths[sheet]
        rels_path = posixpath.join(posixpath.dirname(path), "_rels", posixpath.basename(path) + ".rels")
        rels = xml(parts[rels_path]) if rels_path in parts else ET.Element(f"{{{P}}}Relationships", nsmap={None: P})
        ids = {r.get("Id") for r in rels}
        rel_id = "rIdResearchPictures"
        while rel_id in ids:
            rel_id += "x"
        child(rels, P, "Relationship", Id=rel_id, Type=f"{R}/drawing", Target=posixpath.relpath(drawing_path, posixpath.dirname(path)))
        parts[rels_path] = serialize(rels)
        root = roots[sheet]
        protection = ET.Element(f"{{{S}}}sheetProtection", sheet="1", objects="1", scenarios="0", formatCells="0", formatColumns="0", formatRows="0", insertRows="0", deleteRows="0", sort="0", autoFilter="0", selectLockedCells="0", selectUnlockedCells="0")
        data_index = list(root).index(root.find(f"{{{S}}}sheetData"))
        root.insert(data_index + 1, protection)
        link = ET.Element(f"{{{S}}}drawing")
        link.set(f"{{{R}}}id", rel_id)
        # Drawing precedes legacy drawings, tables and extension lists.
        later = {"legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"}
        insert_at = next((i for i, c in enumerate(root) if ET.QName(c).localname in later), len(root))
        root.insert(insert_at, link)
        parts[path] = serialize(root)
    parts["[Content_Types].xml"] = serialize(content)

    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=output.parent, suffix=".xlsx")
    os.close(fd)
    try:
        with ZipFile(temporary, "w", ZIP_DEFLATED) as archive:
            for name, data in parts.items():
                archive.writestr(name, data)
        os.replace(temporary, output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return {"file": str(output), "pictureCells": len(seen), "storedImages": len(media), "imageBytes": sum(map(len, media.values())), "fileBytes": output.stat().st_size}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    entries = json.loads(args.manifest.read_text())
    for entry in entries:
        if not Path(entry["path"]).is_absolute():
            entry["path"] = str(args.manifest.parent / entry["path"])
    print(json.dumps(embed(args.input, entries, args.output), ensure_ascii=False))


if __name__ == "__main__":
    main()
