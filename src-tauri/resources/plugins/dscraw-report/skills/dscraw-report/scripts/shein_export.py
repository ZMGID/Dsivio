"""Read SHEIN BR trade exports without confusing total and new-customer metrics."""
from datetime import date, datetime, timedelta
from decimal import Decimal, InvalidOperation
import re

import openpyxl

METRICS = ('GMV', '销量', '支付订单数')


def number(value):
    if value is None or str(value).strip() in ('', '-', '/', '—'):
        return None
    text = str(value).strip().removeprefix('BRL').strip()
    percent = text.endswith('%')
    text = text.removesuffix('%')
    if not re.fullmatch(r'[+-]?(?:\d+|\d{1,3}(?:,\d{3})+)(?:\.\d+)?', text):
        raise ValueError('无法识别 SHEIN 数字格式')
    try:
        result = Decimal(text.replace(',', ''))
    except InvalidOperation as error:
        raise ValueError('无效数值') from error
    if not result.is_finite():
        raise ValueError('数值必须有限')
    return result / 100 if percent else result


def parse_export(path, metric, start, end, target):
    if metric not in METRICS:
        raise ValueError('不支持的指标')
    first, last, wanted = map(date.fromisoformat, (start, end, target))
    if (last-first).days != 6 or not first <= wanted <= last:
        raise ValueError('需要覆盖目标日的完整七天窗口')
    workbook = openpyxl.load_workbook(path, read_only=True, data_only=True)
    try:
        if len(workbook.worksheets) != 1:
            raise ValueError('预期一张趋势图工作表')
        workbook.active.reset_dimensions()
        rows = [list(row) for row in workbook.active.values if any(value is not None for value in row)]
    finally:
        workbook.close()
    header = ['统计日期', metric, '新客'+metric, '新客'+metric+'占比']
    if not rows or rows[0] != header:
        raise ValueError('导出表头与当前指标不一致，不能按文件名猜测类型')
    records = []
    for row in rows[1:]:
        if len(row) != 4:
            raise ValueError('数据列数异常')
        day = row[0].date() if isinstance(row[0], datetime) else date.fromisoformat(str(row[0]))
        values = {key: number(value) for key, value in zip(header[1:], row[1:])}
        for key in header[1:3]:
            value = values[key]
            if value is not None and (value < 0 or (metric != 'GMV' and value != value.to_integral_value())):
                raise ValueError('销量/订单必须为非负整数，金额不能为负')
        records.append({'dateIso': day.isoformat(), **values})
    expected = [(first+timedelta(days=i)).isoformat() for i in range(7)]
    if sorted(row['dateIso'] for row in records) != expected:
        raise ValueError('文件日期重复、缺失或与页面区间不一致')
    values = [row[metric] for row in records]
    return {'metric': metric, 'start': start, 'end': end, 'dailyRecords': records,
            'targetValue': next(row[metric] for row in records if row['dateIso'] == target),
            'total': sum(values, Decimal(0)) if all(value is not None for value in values) else None,
            'rawRows': rows}


def reconcile(exports, period_totals, order_metric, single_totals=None):
    if order_metric not in ('unitsSold', 'paidOrders') or set(exports) != set(METRICS):
        raise ValueError('须提供三项导出和明确的数量口径')
    sets = [sorted(row['dateIso'] for row in exports[key]['dailyRecords']) for key in METRICS]
    if any(days != sets[0] for days in sets):
        raise ValueError('三份文件日期集合不同')
    checks, values, reasons = {}, {}, []
    for key in METRICS:
        record = exports[key]
        page = number(period_totals[key])
        period_ok = record['total'] is not None and page == record['total']
        single = number(single_totals[key]) if single_totals else None
        valid = record['targetValue'] is not None and (period_ok or (single is not None and single == record['targetValue']))
        checks[key] = {'fileTotal': record['total'], 'pageTotal': page,
                       'difference': record['total']-page if record['total'] is not None and page is not None else None,
                       'periodMatch': period_ok, 'singlePage': single, 'targetVerified': valid}
        values[key] = record['targetValue'] if valid else None
        if not valid:
            reasons.append(key+'未通过期间或目标单日核验')
    quantity = values['销量' if order_metric == 'unitsSold' else '支付订单数']
    return {'sales': values['GMV'], 'orders': int(quantity) if quantity is not None else None,
            'unitsSold': int(values['销量']) if values['销量'] is not None else None,
            'paidOrders': int(values['支付订单数']) if values['支付订单数'] is not None else None,
            'orderMetric': order_metric, 'checks': checks, 'missingReason': '；'.join(reasons),
            'periodWarnings': [key+'期间合计不同，保留原差额' for key in METRICS if not checks[key]['periodMatch']]}
