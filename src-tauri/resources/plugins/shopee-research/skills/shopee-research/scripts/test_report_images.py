import io
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED

from lxml import etree as ET
from PIL import Image

from prepare_images import prepare
from embed_images import embed, S, R, P, C, X, A


class ReportImagesTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source_image = self.root / "screenshot.png"
        Image.new("RGBA", (600, 1200), (40, 120, 210, 180)).save(self.source_image)
        self.image = self.root / "thumbnail.jpg"
        prepare(self.source_image, self.image)
        self.base = self.root / "base.xlsx"
        parts = {
            "[Content_Types].xml": f'<Types xmlns="{C}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>',
            "_rels/.rels": f'<Relationships xmlns="{P}"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
            "xl/workbook.xml": f'<workbook xmlns="{S}" xmlns:r="{R}"><sheets><sheet name="Products" sheetId="1" r:id="rId1"/></sheets></workbook>',
            "xl/_rels/workbook.xml.rels": f'<Relationships xmlns="{P}"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>',
            "xl/styles.xml": f'<styleSheet xmlns="{S}"><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs></styleSheet>',
            "xl/worksheets/sheet1.xml": f'<worksheet xmlns="{S}"><sheetData><row r="7"><c r="A7" t="inlineStr"><is><t>SKU-007</t></is></c><c r="B7"/><c r="C7"><f>2*3</f><v>6</v></c></row><row r="8"><c r="B8"/></row></sheetData></worksheet>',
        }
        with ZipFile(self.base, "w", ZIP_DEFLATED) as archive:
            for name, data in parts.items():
                archive.writestr(name, data)

    def entry(self, cell):
        return {"sheet": "Products", "cell": cell, "productId": "007", "path": str(self.image)}

    def test_compress_keeps_aspect_and_source(self):
        self.assertLessEqual(self.image.stat().st_size, 40 * 1024)
        with Image.open(self.source_image) as original, Image.open(self.image) as thumbnail:
            self.assertEqual(original.size, (600, 1200))
            self.assertEqual(thumbnail.size, (320, 320))
            self.assertEqual(thumbnail.format, "JPEG")
            self.assertGreater(min(thumbnail.getpixel((2, 160))), 245)
            self.assertLess(thumbnail.getpixel((160, 160))[0], 200)

    def test_compatible_drawings_deduplicate_lock_and_preserve_data(self):
        output = self.root / "report.xlsx"
        result = embed(self.base, [self.entry("B7"), self.entry("B8")], output)
        self.assertEqual(result["pictureCells"], 2)
        self.assertEqual(result["storedImages"], 1)
        with ZipFile(self.base) as before, ZipFile(output) as after:
            old = ET.fromstring(before.read("xl/worksheets/sheet1.xml"))
            new = ET.fromstring(after.read("xl/worksheets/sheet1.xml"))
            for cell in ("A7", "C7"):
                original = old.find(f".//{{{S}}}c[@r='{cell}']")
                actual = ET.fromstring(ET.tostring(new.find(f".//{{{S}}}c[@r='{cell}']")))
                actual.attrib.pop("s", None)
                self.assertEqual(ET.tostring(original), ET.tostring(actual))
            for name in before.namelist():
                if name not in {"[Content_Types].xml", "xl/styles.xml", "xl/worksheets/sheet1.xml"}:
                    self.assertEqual(before.read(name), after.read(name))
            self.assertNotIn("xl/metadata.xml", after.namelist())
            self.assertEqual(new.find(f"{{{S}}}sheetProtection").get("objects"), "1")
            styles = ET.fromstring(after.read("xl/styles.xml")).find(f"{{{S}}}cellXfs")
            for cell in new.iter(f"{{{S}}}c"):
                self.assertEqual(styles[int(cell.get("s"))].find(f"{{{S}}}protection").get("locked"), "0")
            drawing = ET.fromstring(after.read("xl/drawings/research1.xml"))
            relationships = {r.get("Id"): r.get("Target") for r in ET.fromstring(after.read("xl/drawings/_rels/research1.xml.rels"))}
            self.assertEqual(len(drawing), 2)
            for row, anchor in zip((6, 7), drawing):
                self.assertEqual(anchor.tag, f"{{{X}}}twoCellAnchor")
                self.assertIsNone(anchor.get("editAs"))
                self.assertEqual(anchor.find(f"{{{X}}}from/{{{X}}}row").text, str(row))
                self.assertEqual(anchor.find(f"{{{X}}}to/{{{X}}}row").text, str(row))
                self.assertEqual(anchor.find(f"{{{X}}}to/{{{X}}}colOff").text, "1504950")
                self.assertEqual(anchor.find(f"{{{X}}}to/{{{X}}}rowOff").text, "1504950")
                self.assertEqual(anchor.find(f".//{{{A}}}picLocks").get("noMove"), "1")
                rel = anchor.find(f".//{{{A}}}blip").get(f"{{{R}}}embed")
                media_path = "xl/" + relationships[rel].removeprefix("../")
                self.assertEqual(after.read(media_path), self.image.read_bytes())
                with Image.open(io.BytesIO(after.read(media_path))) as picture:
                    self.assertEqual(picture.size, (320, 320))

    def test_rejects_duplicate_destination_and_overwrite(self):
        output = self.root / "report.xlsx"
        output.write_bytes(b"keep this file")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            embed(self.base, [self.entry("B7"), self.entry("B7")], output)
        self.assertEqual(output.read_bytes(), b"keep this file")
        with self.assertRaisesRegex(ValueError, "empty"):
            embed(self.base, [self.entry("A7")], output)
        self.assertEqual(output.read_bytes(), b"keep this file")


if __name__ == "__main__":
    unittest.main()
