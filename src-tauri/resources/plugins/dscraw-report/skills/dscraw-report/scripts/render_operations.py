"""Render the internal HTML from this run's archived exports and verified report data."""
import argparse
import csv
import html
from datetime import date, datetime, timedelta
from decimal import Decimal
import json
from pathlib import Path

import openpyxl
from report_state import ROOT, paths, read_json, require


def number(value, style='br'):
    if value is None or str(value).strip() in ('', '-'):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    text = str(value).strip().rstrip('%')
    if style == 'br':
        text = text.replace('.', '').replace(',', '.')
    result = Decimal(text)
    require(result.is_finite(), '导出数值无效')
    return float(result)


def iso_day(value):
    value = str(value)
    return datetime.strptime(value, '%d/%m/%Y').date().isoformat() if '/' in value else date.fromisoformat(value).isoformat()


def sheet_rows(path, sheet=None):
    # TK exports can declare an A1 dimension despite containing a full sheet.
    book = openpyxl.load_workbook(path, read_only=False, data_only=True)
    try:
        ws = book[sheet] if sheet else book.worksheets[0]
        return [list(r) for r in ws.iter_rows(values_only=True) if any(v is not None for v in r)]
    finally:
        book.close()


def daily_rows(rows):
    header = next(r for r in rows if r[0] in ('日期', '统计日期'))
    for r in rows:
        try:
            day = iso_day(r[0])
        except (ValueError, TypeError):
            continue
        yield day, dict(zip(header, r))


def operating_rows(platform, files):
    result = []
    if platform == 'shopee':
        mapping = {'sales': '销售额 (BRL)', 'orders': '订单数', 'aov': '每个订单的销售额',
                   'visitors': '访客数', 'clicks': '商品点击量', 'cvr': '订单转化率'}
        for day, row in daily_rows(sheet_rows(files['店铺经营']['path'], '已下订单')):
            result.append({'date': day, **{k: number(row.get(v)) for k, v in mapping.items()}})
    elif platform == 'tiktok':
        rows = sheet_rows(files['关键指标']['path'])
        start = next(i for i, r in enumerate(rows) if r[0] == '每日数据')
        mapping = {'sales': 'GMV', 'orders': '订单数', 'units': '商品成交件数', 'aov': '平均订单金额',
                   'visitors': '商品访客数', 'clicks': '商品点击量', 'cvr': '转化率',
                   'liveGmv': '达人直播归因 GMV', 'videoGmv': '联盟视频归因 GMV'}
        for day, row in daily_rows(rows[start + 1:]):
            parsed = {k: number(row.get(v)) for k, v in mapping.items()}
            if parsed['cvr'] is not None:
                parsed['cvr'] *= 100
            result.append({'date': day, **parsed})
    elif platform == 'shein':
        series = {kind: dict(daily_rows(sheet_rows(files[kind]['path']))) for kind in ('GMV', '销量', '支付订单数')}
        days = set(series['GMV']) | set(series['销量']) | set(series['支付订单数'])
        for day in sorted(days):
            sales = number(series['GMV'].get(day, {}).get('GMV'))
            orders = number(series['支付订单数'].get(day, {}).get('支付订单数'))
            result.append({'date': day, 'sales': sales, 'orders': orders,
                           'units': number(series['销量'].get(day, {}).get('销量')),
                           'aov': sales / orders if sales is not None and orders else None})
    return result


def advertising_rows(manifest, day):
    rows = list(csv.reader(Path(manifest['path']).read_text(encoding='utf-8-sig').splitlines()))
    period = next(r[1] for r in rows if r and r[0] == '时间')
    endpoints = [iso_day(v.strip()) for v in period.split(' - ')]
    require(endpoints == [day, day], '广告导出不是目标单日，暂不展示为单日明细')
    head = next(i for i, r in enumerate(rows) if '广告 / 商品名称' in r)
    result = []
    mapping = {'spend': '花费', 'sales': '销售金额', 'roas': '广告支出回报率',
               'conversions': '转化', 'clicks': '点击数', 'ctr': '点击率'}
    for line, row in enumerate(rows[head + 1:], head + 2):
        if not row:
            continue
        require(len(row) == len(rows[head]), '广告 CSV 列数变化，需核对表头')
        r = dict(zip(rows[head], row))
        record = {k: number(r.get(v), 'en') for k, v in mapping.items()}
        record.update(line=line, name=r['广告 / 商品名称'], status=r['状态'], productId=r['商品编号'],
                      level='group' if r['状态'] != '-' and r['竞价方式'] != '-' else 'product')
        record['cpa'] = record['spend'] / record['conversions'] if record['spend'] is not None and record['conversions'] else None
        result.append(record)
    return result


def change(current, previous):
    return (current / previous - 1) * 100 if current is not None and previous else None


def findings(stores):
    result = []
    for s in stores:
        a, b = s['current'], s['previous']
        delta = s['change']
        if delta is None:
            continue
        title = '销售额较前一天上升' if delta >= 0 else '销售额较前一天下降'
        evidence = f"销售额 {b['sales']:,.2f} → {a['sales']:,.2f} {s['currency']}（{delta:+.2f}%）。"
        action = '结合当天活动、价格和可售商品检查变化原因，并继续观察后续表现。汇总数据不能单独确定原因。'
        if all(r.get(k) is not None for r in (a, b) for k in ('visitors', 'cvr')):
            evidence += f"访客 {b['visitors']:,.0f} → {a['visitors']:,.0f}；转化率 {b['cvr']:.2f}% → {a['cvr']:.2f}%。"
            if a['visitors'] > b['visitors'] and a['cvr'] < b['cvr']:
                title = '流量增加，转化率下降'
                action = '优先检查新增流量对应的商品、价格优惠和可售规格；不单凭流量变化决定追加预算。'
        elif a.get('units') is not None and b.get('units') is not None:
            evidence += f"销量 {b['units']:.0f} → {a['units']:.0f} 件。"
        result.append({'tone': 'teal' if delta >= 0 else 'amber', 'tag': '经营变化 · ' + s['name'],
                       'title': title, 'evidence': evidence, 'action': action, 'magnitude': abs(delta)})
    return sorted(result, key=lambda r: r['magnitude'], reverse=True)[:4]


def build_data(report):
    day = report['dateIso']
    previous_day = (date.fromisoformat(day) - timedelta(days=1)).isoformat()
    manifests = [read_json(p) for p in report.get('downloads', [])]
    configs = {s['storeId']: s for s in report['configSnapshot']['stores']}
    stores = []
    for source in report['stores']:
        config = configs[source['storeId']]
        files = {}
        for item in sorted(manifests, key=lambda m: m.get('collectedAt', '')):
            if item['storeId'] == source['storeId']:
                files[item['type']] = item
        s = {'id': source['storeId'], 'name': source['name'], 'platform': source['platform'],
             'currency': source['currency'], 'orderMetric': source['orderMetric'], 'daily': [], 'ads': [],
             'sources': [{'type': m['type'], 'start': m['start'], 'end': m['end'], 'file': Path(m['path']).name} for m in files.values()],
             'notes': list(filter(None, [source.get('missingReason'), source.get('adMissingReason')]))}
        try:
            s['daily'] = operating_rows(s['platform'], files)
            require(len({r['date'] for r in s['daily']}) == len(s['daily']), '每日数据日期重复')
            target = next((r for r in s['daily'] if r['date'] == day), {})
            for key, raw_key in [('sales', 'sales'), ('units' if s['orderMetric'] == 'unitsSold' else 'orders', 'orders')]:
                if target.get(key) is not None and source.get(raw_key) is not None:
                    require(abs(target[key] - float(source[raw_key])) < .01, '原始导出与已核验日报不一致，趋势暂不使用')
        except (OSError, KeyError, ValueError, StopIteration) as error:
            s['notes'].append('经营导出不可用：' + str(error))
            s['daily'] = []
        if config['collectAds']:
            s['adTotal'] = {k: number(source.get(v), 'en') for k, v in [('spend', 'adCost'), ('sales', 'adSales'), ('roas', 'roi')]}
            try:
                s['ads'] = advertising_rows(files['广告组数据'], day)
            except (OSError, KeyError, ValueError, StopIteration) as error:
                s['notes'].append('广告明细不可用：' + str(error))
        s['daily'].sort(key=lambda r: r['date'])
        s['current'] = dict(next((r for r in s['daily'] if r['date'] == day), {'date': day}))
        # Verified report values govern the target day, including explicit missing values.
        s['current']['sales'] = number(source.get('sales'), 'en')
        quantity = 'units' if s['orderMetric'] == 'unitsSold' else 'orders'
        s['current'][quantity] = number(source.get('orders'), 'en')
        s['previous'] = next((r for r in s['daily'] if r['date'] == previous_day), {})
        s['change'] = change(s['current'].get('sales'), s['previous'].get('sales'))
        s['aovBasis'] = '客单价按订单口径，沿用平台导出值' if s['platform'] != 'shein' else '客单价 = GMV / 支付订单数'
        stores.append(s)
    currency = {s['currency'] for s in stores}
    single = len(currency) == 1
    def total(values):
        return float(sum(Decimal(str(v)) for v in values)) if values and all(v is not None for v in values) else None
    sales = total([s['current'].get('sales') for s in stores]) if single else None
    previous = total([s['previous'].get('sales') for s in stores]) if single else None
    ads = [s['adTotal'].get('spend') for s in stores if 'adTotal' in s]
    common = set.intersection(*[{r['date'] for r in s['daily'] if r['date'] <= day} for s in stores]) if stores else set()
    return {'date': day, 'title': report['configSnapshot']['report']['title'], 'currency': next(iter(currency)) if single else '多币种',
            'stores': stores, 'total': sales, 'previous': previous, 'change': change(sales, previous),
            'adSpend': total(ads) if single else None, 'adStoreCount': len(ads), 'sourceCount': len(manifests),
            'commonDates': sorted(common), 'previousDate': previous_day, 'findings': findings(stores)}


def render(report, template_name=None, output_path=None):
    config = report['configSnapshot']['report']
    template_name = template_name or config.get('operationsTemplate')
    if not template_name:
        return None
    template = (ROOT / 'templates' / (template_name + '.html')).read_text(encoding='utf-8')
    data = build_data(report)
    common = data['commonDates']
    labels = {'__TITLE__': data['title'], '__DAY__': data['date'], '__PREVIOUS__': data['previousDate'],
              '__STORE_COUNT__': len(data['stores']), '__SOURCE_COUNT__': data['sourceCount'], '__CURRENCY__': data['currency'],
              '__RANGE__': f'共同覆盖日期 · {common[0]}—{common[-1]}' if common else '暂无共同完整日期'}
    for key, value in labels.items():
        template = template.replace(key, html.escape(str(value), quote=True))
    output = Path(output_path or paths(report['configSnapshot'], report['dateIso'])['operationsHtml'])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(template.replace('__DATA__', json.dumps(data, ensure_ascii=False, allow_nan=False).replace('<', '\\u003c')), encoding='utf-8')
    return str(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_json')
    args = parser.parse_args()
    print(json.dumps({'operationsHtml': render(read_json(args.report_json))}, ensure_ascii=False))
