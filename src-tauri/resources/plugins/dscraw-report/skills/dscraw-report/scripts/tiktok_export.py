"""Strict parser for TikTok BR Chinese key-metrics daily exports."""
from datetime import date, datetime, timedelta
from decimal import Decimal, InvalidOperation
import re

import openpyxl


def number(value):
    if value is None or str(value).strip() in ('', '-', '/', '—'):
        return None
    text = str(value).strip().replace('\u00a0', '').replace(' ', '').replace('R$', '')
    percent = text.endswith('%')
    text = text.removesuffix('%')
    if ',' in text and '.' in text:
        if text.rfind(',') > text.rfind('.'):
            text = text.replace('.', '').replace(',', '.')
        else:
            text = text.replace(',', '')
    elif ',' in text:
        text = text.replace(',', '.')
    try:
        result = Decimal(text)
    except InvalidOperation as error:
        raise ValueError(f'无法识别数值: {value!r}') from error
    if not result.is_finite():
        raise ValueError('数值必须有限')
    return result / 100 if percent else result


def day(value):
    return datetime.strptime(str(value).strip(), '%d/%m/%Y').date()


def parse_export(path, target_date, expected_start=None, expected_end=None):
    target = date.fromisoformat(target_date)
    workbook = openpyxl.load_workbook(path, read_only=True, data_only=True)
    try:
        sheet = workbook.active
        sheet.reset_dimensions()  # TikTok sometimes declares A1 for the entire sheet.
        rows = [list(row) for row in sheet.iter_rows(values_only=True)]
    finally:
        workbook.close()
    metadata = [str(cell) for row in rows for cell in row
                if isinstance(cell, str) and cell.startswith('分析日期')]
    if len(metadata) != 1:
        raise ValueError('分析日期元数据缺失或不唯一')
    dates = re.findall(r'\d{2}/\d{2}/\d{4}', metadata[0])
    if len(dates) != 2:
        raise ValueError('需要七天导出的明确起止日期')
    start, end = map(day, dates)
    if not start <= target <= end or (end - start).days != 6:
        raise ValueError('导出不是覆盖目标日的七天窗口')
    if expected_start and start.isoformat() != expected_start:
        raise ValueError('文件开始日期与页面不一致')
    if expected_end and end.isoformat() != expected_end:
        raise ValueError('文件结束日期与页面不一致')

    def block(label):
        indexes = [i for i, row in enumerate(rows) if row and row[0] == label]
        if len(indexes) != 1:
            raise ValueError(f'{label}区块缺失或不唯一，小时数据不能替代每日数据')
        return indexes[0]

    def header(index):
        values = rows[index + 1]
        names = values[1:]
        if any(not isinstance(name, str) or not name for name in names) or len(set(names)) != len(names):
            raise ValueError('指标表头缺失或重复')
        if not {'GMV', '订单数'}.issubset(names):
            raise ValueError('缺少 GMV 或精确订单数列')
        return values

    overview, daily = block('数据概览'), block('每日数据')
    overview_header, daily_header = header(overview), header(daily)
    total_rows = [row for row in rows[overview + 2:daily] if row and row[0] == '总计值']
    if len(total_rows) != 1:
        raise ValueError('总计值缺失或不唯一')

    def metrics(values, names):
        if len(values) != len(names):
            raise ValueError('指标列数与表头不一致')
        return {name: number(value) for name, value in zip(names[1:], values[1:])}

    totals = metrics(total_rows[0], overview_header)
    records = []
    for row in rows[daily + 2:]:
        if not row or all(value is None for value in row):
            continue
        records.append({'dateIso': day(row[0]).isoformat(), **metrics(row, daily_header)})
    expected = [(start + timedelta(days=i)).isoformat() for i in range(7)]
    if sorted(record['dateIso'] for record in records) != expected:
        raise ValueError('每日日期重复、缺失或不匹配分析区间')
    selected = next(record for record in records if record['dateIso'] == target_date)
    reasons, checked = [], {}
    for field in ('GMV', '订单数'):
        values = [record[field] for record in records]
        valid = all(value is not None and value >= 0 for value in values)
        if field == '订单数':
            valid = valid and all(value == value.to_integral_value() for value in values if value is not None)
        checked[field] = valid and totals[field] is not None and sum(values, Decimal(0)) == totals[field]
        if not checked[field]:
            reasons.append(f'{field}缺失或每日合计不等于文件总计；需单日交叉核验')
    return {
        'start': start.isoformat(), 'end': end.isoformat(), 'dateIso': target_date,
        'sales': selected['GMV'] if checked['GMV'] else None,
        'orders': int(selected['订单数']) if checked['订单数'] else None,
        'missingReason': '；'.join(reasons), 'checks': checked,
        'totals': totals, 'dailyRecords': records, 'rawRows': rows,
    }
