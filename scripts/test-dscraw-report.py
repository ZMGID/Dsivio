"""Verify external daily-report configuration without opening a store."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SKILL = Path(__file__).resolve().parents[1] / 'src-tauri/resources/plugins/dscraw-report/skills/dscraw-report'


class ExternalReportConfigTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.data = Path(self.temp.name) / '运营选择的数据目录'
        self.config = self.data / '配置/我的日报.json'
        state = json.loads((SKILL / 'examples/日报配置示例.json').read_text())
        state['report']['outputRoot'] = str(self.data / '我的日报')
        state['stores'][0]['enabled'] = True
        state['stores'][0]['name'] = '测试店铺'
        self.config.parent.mkdir(parents=True)
        self.config.write_text(json.dumps(state, ensure_ascii=False), encoding='utf-8')
        self.original = self.config.read_bytes()
        self.day = '2026-10-06'
        self.capture = self.data / '我的日报/数据' / self.day / 'Shopee采集/10001/capture.json'

    def command(self, script, *args):
        return subprocess.run([sys.executable, '-B', str(SKILL / 'scripts' / script), *args],
                              capture_output=True, text=True, encoding='utf-8',
                              env={**os.environ, 'PYTHONUTF8': '1'}, cwd=self.temp.name)

    def write_capture(self, day=None):
        self.capture.parent.mkdir(parents=True, exist_ok=True)
        self.capture.write_text(json.dumps({
            'dateIso': day or self.day, 'sales': '123.45', 'orders': 8,
            'capturedAt': '2026-10-07T09:00:00+08:00', 'downloads': [],
            'businessExport': {'daily': {self.day: {'sales': '123.45', 'orders': 8}}},
        }), encoding='utf-8')

    def test_plan_and_assemble_use_selected_external_directory(self):
        result = self.command('report_state.py', 'plan', '--config', str(self.config), '--date', self.day)
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout)
        self.assertEqual(plan['configSnapshot']['configFile'], str(self.config.resolve()))
        self.assertEqual([row['storeId'] for row in plan['configSnapshot']['stores']], ['10001'])
        self.write_capture()
        result = self.command('assemble_report.py', '--config', str(self.config), '--date', plan['dateIso'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), plan['paths']['json'])
        report = json.loads(Path(plan['paths']['json']).read_text())
        self.assertEqual(report['dateIso'], self.day)
        self.assertEqual(report['configSnapshot']['configFile'], str(self.config.resolve()))
        self.assertEqual([(row['storeId'], row['sales'], row['orders']) for row in report['stores']],
                         [('10001', '123.45', 8)])
        self.assertEqual(self.config.read_bytes(), self.original)

    def test_missing_capture_remains_missing_instead_of_zero(self):
        result = self.command('assemble_report.py', '--config', str(self.config), '--date', self.day)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(Path(result.stdout.strip()).read_text())
        self.assertIsNone(report['stores'][0]['sales'])
        self.assertIsNone(report['stores'][0]['orders'])
        self.assertEqual(report['stores'][0]['status'], '未获取')

    def test_advertising_revenue_survives_assembly_from_both_platforms(self):
        state = json.loads(self.original)
        for platform, field in [('shopee', 'attributedSales'), ('tiktok', 'adRevenue')]:
            with self.subTest(platform=platform):
                state['stores'][0].update(platform=platform, collectAds=True)
                self.config.write_text(json.dumps(state, ensure_ascii=False), encoding='utf-8')
                folder = 'Shopee采集' if platform == 'shopee' else 'TK采集'
                capture = self.data / '我的日报/数据' / self.day / folder / '10001/capture.json'
                capture.parent.mkdir(parents=True, exist_ok=True)
                capture.write_text(json.dumps({
                    'dateIso': self.day, 'sales': '123.45', 'orders': 8,
                    'capturedAt': '2026-10-07T09:00:00+08:00', 'downloads': [],
                    'adCost': '20.00', 'roi': '3.00', field: '60.00',
                }), encoding='utf-8')
                result = self.command('assemble_report.py', '--config', str(self.config), '--date', self.day)
                self.assertEqual(result.returncode, 0, result.stderr)
                report = json.loads(Path(result.stdout.strip()).read_text())
                self.assertEqual(report['stores'][0].get('adSales'), '60.00')

    def test_wrong_capture_day_does_not_overwrite_formal_report(self):
        self.write_capture()
        result = self.command('assemble_report.py', '--config', str(self.config), '--date', self.day)
        self.assertEqual(result.returncode, 0, result.stderr)
        output = Path(result.stdout.strip())
        original = output.read_bytes()
        self.write_capture('2026-10-05')
        result = self.command('assemble_report.py', '--config', str(self.config), '--date', self.day)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(output.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
