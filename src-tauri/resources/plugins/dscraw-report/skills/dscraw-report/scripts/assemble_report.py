"""Assemble verified per-store captures into one formal report JSON."""
import argparse
from pathlib import Path

from report_state import load_snapshot, paths, prepare_report, read_json, write_json


FOLDERS = {'shopee': 'Shopee采集', 'tiktok': 'TK采集', 'shein': 'SHEIN采集'}


def integer(value):
    return None if value is None else int(float(value))


def shopee_records(capture, target):
    rows = []
    for day, value in capture.get('businessExport', {}).get('daily', {}).items():
        row = {**value, 'dateIso': day, 'source': 'excel-daily-row'}
        if day == target:
            row['sales'], row['orders'] = capture.get('sales'), capture.get('orders')
            row['source'] = capture.get('source') or 'excel-target-day'
        rows.append(row)
    return sorted(rows, key=lambda row: row['dateIso'])


def tiktok_records(capture, target):
    rows = capture.get('dailyRecords', [])
    for row in rows:
        row.setdefault('source', 'excel-target-day' if row.get('dateIso') == target else 'excel-daily-row')
    return rows


def shein_records(capture, target):
    combined = {}
    for metric in ('GMV', '销量', '支付订单数'):
        for source in capture.get('exports', {}).get(metric, {}).get('dailyRecords', []):
            combined.setdefault(source['dateIso'], {'dateIso': source['dateIso']}).update(source)
    for row in combined.values():
        row['sales'] = row.get('GMV')
        row['unitsSold'] = integer(row.get('销量'))
        row['paidOrders'] = integer(row.get('支付订单数'))
        row['source'] = 'excel-target-day' if row['dateIso'] == target else 'excel-daily-row'
    return [combined[key] for key in sorted(combined)]


def store_result(config, capture, target):
    platform = config['platform']
    if platform == 'shopee':
        records = shopee_records(capture, target)
    elif platform == 'tiktok':
        records = tiktok_records(capture, target)
    else:
        records = shein_records(capture, target)
    target_row = next((row for row in records if row.get('dateIso') == target), {})
    result = {
        'storeId': config['storeId'],
        'sales': capture.get('sales'),
        'orders': capture.get('orders'),
        'missingReason': capture.get('missingReason') or '',
        'capturedAt': capture.get('capturedAt'),
        'source': capture.get('source') or 'excel-target-day',
        'dailyRecords': records,
    }
    if platform == 'tiktok':
        result['itemsSold'] = integer(target_row.get('商品成交件数'))
    if platform == 'shein':
        result['unitsSold'] = integer(capture.get('unitsSold'))
        result['paidOrders'] = integer(capture.get('paidOrders'))
    if config['collectAds']:
        result.update(adCost=capture.get('adCost'), roi=capture.get('roi'))
        revenue_key = 'adRevenue' if platform == 'tiktok' else 'attributedSales'
        result['adSales'] = capture.get(revenue_key)
        if result['adCost'] is None or result['roi'] is None:
            result['adMissingReason'] = capture.get('adMissingReason') or '目标日广告 Excel 或页面核验未完成'
        else:
            result['adSource'] = capture.get('adSource')
            result['adSourceReason'] = capture.get('adSourceReason')
    return result


def assemble(report_name, target, config_path=None):
    snapshot = load_snapshot(config_path, report_id=report_name)
    root = Path(snapshot['report']['outputRoot'])
    stores, downloads, sources = [], [], []
    for config in snapshot['stores']:
        capture_path = root / '数据' / target / FOLDERS[config['platform']] / config['storeId'] / 'capture.json'
        if not capture_path.is_file():
            stores.append({'storeId': config['storeId'], 'sales': None, 'orders': None,
                           'missingReason': '未找到目标日采集结果',
                           **({'adCost': None, 'roi': None, 'adMissingReason': '经营采集未完成'} if config['collectAds'] else {})})
            continue
        capture = read_json(capture_path)
        if capture.get('dateIso') != target:
            raise ValueError(f"{config['name']} 采集日期不是 {target}")
        stores.append(store_result(config, capture, target))
        downloads.extend(capture.get('downloads', []))
        sources.append(f"{config['name']}: 已核验目标日 {target} 的 Excel 归档和采集证据")
    report = prepare_report({'dateIso': target, 'downloads': list(dict.fromkeys(downloads)),
                             'sources': sources, 'stores': stores}, snapshot)
    output = paths(snapshot, target)['json']
    write_json(output, report)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    config = parser.add_mutually_exclusive_group(required=True)
    config.add_argument('--report', help='兼容旧 Skill 目录内的日报配置名')
    config.add_argument('--config', help='用户数据目录中的日报配置绝对路径')
    parser.add_argument('--date', required=True)
    args = parser.parse_args()
    print(assemble(args.report, args.date, args.config))


if __name__ == '__main__':
    main()
