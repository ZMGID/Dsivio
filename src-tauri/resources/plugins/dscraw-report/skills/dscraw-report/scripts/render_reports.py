"""Generate every configured report from one collected daily dataset."""
import argparse
import json
from pathlib import Path
import subprocess

from report_state import ROOT, output_paths, read_json, report_outputs
from render_operations import render as render_operations


def render_all(report_json, node='node'):
    report = read_json(report_json)
    config = report['configSnapshot']['report']
    outputs = output_paths(config, report['dateIso'])
    for item in report_outputs(config):
        files = outputs[item['id']]
        if item['renderer'] == 'operations':
            render_operations(report, item['template'], files['html'])
        else:
            selected = {'template': item['template'], **files}
            result = subprocess.run([node, str(ROOT / 'scripts/render-report.cjs'), str(Path(report_json).resolve()),
                                     '--output', json.dumps(selected, ensure_ascii=False)], capture_output=True, text=True, encoding='utf-8')
            if result.returncode:
                raise ValueError(result.stderr or result.stdout)
    return outputs


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_json')
    parser.add_argument('--node', default='node', help='Node executable path when node is not on PATH')
    args = parser.parse_args()
    print(json.dumps({'reports': render_all(args.report_json, args.node)}, ensure_ascii=False, indent=2))
