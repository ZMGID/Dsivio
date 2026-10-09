"""Exercise both packaged report entry points with links and long opportunity IDs.

Run with the report requirements installed:
  uv run --with-requirements src-tauri/resources/plugins/shopee-research/skills/shopee-research/scripts/requirements.txt python scripts/test-product-sku-report.py
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from openpyxl import load_workbook
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


class ReportTest(unittest.TestCase):
    def test_packaged_reports_preserve_lookup_identifiers(self):
        for plugin in ('shopee-research', 'product-sku-search'):
            for identifiers in (['url'], ['opportunityId'], ['url', 'opportunityId']):
                with self.subTest(plugin=plugin, identifiers=identifiers), tempfile.TemporaryDirectory() as tmp:
                    directory = Path(tmp)
                    Image.new('RGB', (100, 100), 'white').save(directory / 'product.png')
                    columns = ['name', 'image', 'package', 'price', *identifiers]
                    candidate = dict(id='test', name='Product', unit='件', salesUnit='件', qty=1,
                                     price=19.9, image='product.png',
                                     opportunityId='12345678901234567890', url='https://example.com/product')
                    for key in ('url', 'opportunityId'):
                        if key not in identifiers:
                            del candidate[key]
                    data = dict(title='Report', summary='Summary', analysis='Analysis', currency='BRL',
                                columns=columns, candidates=[candidate])
                    source = directory / 'report.json'
                    source.write_text(json.dumps(data), encoding='utf-8')
                    output = directory / 'report.xlsx'
                    script = ROOT / f'src-tauri/resources/plugins/{plugin}/skills/{plugin}/scripts/build_report.py'
                    result = subprocess.run([sys.executable, str(script), str(source), str(output)], capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    wb = load_workbook(output)
                    sheet = wb.active
                    for key in identifiers:
                        cell = sheet.cell(7, columns.index(key) + 1)
                        self.assertEqual(cell.value, candidate[key])
                        self.assertEqual(cell.data_type, 's')
                        if key == 'url':
                            self.assertEqual(cell.hyperlink.target, candidate[key])
                    self.assertEqual(len(sheet._images), 1)
                    wb.close()


    def test_setup_records_an_interpreter_that_already_ships_the_report_libraries(self):
        # `dsivio python` (bundled Python 3.12) already has openpyxl/Pillow/lxml: no venv, pip or network.
        for plugin in ('shopee-research', 'product-sku-search'):
            script = ROOT / f'src-tauri/resources/plugins/{plugin}/skills/{plugin}/scripts/setup_runtime.py'
            with self.subTest(plugin=plugin), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp) / 'runtime'
                checked = subprocess.run([sys.executable, str(script), '--root', str(root), '--check'],
                                         capture_output=True, text=True)
                self.assertEqual(checked.returncode, 0, checked.stderr)
                self.assertTrue(json.loads(checked.stdout)['ready'])
                self.assertFalse(root.exists(), '--check must not write')
                result = subprocess.run([sys.executable, str(script), '--root', str(root)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                reported = json.loads(result.stdout)
                self.assertEqual(Path(reported['python']), Path(sys.executable))
                self.assertEqual(set(reported['versions']), {'openpyxl', 'Pillow', 'lxml'})
                self.assertFalse((root / 'venv').exists(), 'no private venv when the interpreter is ready')
                self.assertEqual(json.loads((root / 'runtime.json').read_text(encoding='utf-8'))['python'], reported['python'])

    def test_bundled_interpreter_is_recognised_by_its_runtime_directory(self):
        sys.path.insert(0, str(ROOT / 'src-tauri/resources/plugins/shopee-research/skills/shopee-research/scripts'))
        try:
            import setup_runtime
        finally:
            sys.path.pop(0)
        self.assertTrue(setup_runtime.is_bundled('/Applications/Dsivio.app/Contents/Resources/video-runtime/python/bin/python3'))
        self.assertTrue(setup_runtime.is_bundled(str(Path('C:/Users/me/AppData/Local/dsivio/video-runtime/python/python.exe'))))
        self.assertFalse(setup_runtime.is_bundled('/usr/bin/python3'))


if __name__ == '__main__':
    unittest.main()
