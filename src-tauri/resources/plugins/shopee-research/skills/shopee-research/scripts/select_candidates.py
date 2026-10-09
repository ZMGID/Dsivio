"""Select representative products using displayed sales magnitudes."""
import argparse
import json
import math
import re
from pathlib import Path


def sales_value(raw):
    text = str(raw).strip().lower()
    match = re.fullmatch(r'([\d.,]+)\s*(mil|k|m|万|千)?\s*\+?\s*(?:vendidos?|vendido\(s\)|sold|已售)?', text)
    if not match:
        return None
    number, unit = match.groups()
    if unit:
        number = number.replace(',', '.')
    else:
        number = re.sub(r'[.,](?=\d{3}(?:[.,]|$))', '', number)
    try:
        value = float(number) * {'mil': 1000, 'k': 1000, 'm': 1000000, '万': 10000, '千': 1000, None: 1}[unit]
        return value if math.isfinite(value) and value >= 0 else None
    except ValueError:
        return None


def select(rows):
    unique = {}
    for row in rows:
        if row.get('included', True) is False:
            continue
        price = row.get('price')
        sales = sales_value(row.get('salesRaw', ''))
        if not isinstance(price, (int, float)) or isinstance(price, bool) or not math.isfinite(price) or price <= 0 or sales is None:
            continue
        product_id = str(row['id'])
        unique.setdefault(product_id, dict(row, salesMagnitude=sales))
    eligible = list(unique.values())
    if len({r.get('period', '累计') for r in eligible}) > 1:
        raise ValueError('Compare one sales period at a time')
    if len({r.get('currency') for r in eligible}) > 1:
        raise ValueError('Compare one currency at a time')
    if not eligible:
        return []
    ordered = sorted(eligible, key=lambda r: (r['salesMagnitude'], str(r['id'])))
    highest = min(eligible, key=lambda r: (-r['salesMagnitude'], str(r['id'])))
    lowest = min((r for r in eligible if r['salesMagnitude'] > 0),
                 key=lambda r: (r['price'], -r['salesMagnitude'], str(r['id'])), default=None)
    result = {}
    for role, row in [('销量最高', highest), ('销量居中', ordered[(len(ordered) - 1) // 2]),
                      ('最低标价且有销量', lowest)]:
        if row is not None:
            item = result.setdefault(str(row['id']), dict(row, roles=[]))
            item['roles'].append(role)
    return list(result.values())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('samples', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(select(json.loads(args.samples.read_text(encoding='utf-8'))), ensure_ascii=False, indent=2), encoding="utf-8")
