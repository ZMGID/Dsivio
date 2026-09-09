"""Read media rates from Dsivio's existing model database, never a separate rate card."""
import json
import os
from decimal import Decimal
from pathlib import Path


def media_pricing(model):
    try:
        raw = os.environ.get('DSIVIO_MEDIA_PRICING')
        if raw is not None:
            catalog = json.loads(raw)
        else:
            # Source checkout / standalone development. Packaged hosts inject the merged catalog.
            path = next((p / 'src/data/modelDatabase.json' for p in Path(__file__).resolve().parents
                         if (p / 'src/data/modelDatabase.json').is_file()), None)
            catalog = {k: v.get('mediaPricing', {}) for k, v in json.loads(path.read_text()).items() if isinstance(v, dict)} if path else {}
        key = model.lower()
        catalog = {k.lower(): v for k, v in catalog.items()}
        return catalog.get(key) or catalog.get(key.removeprefix('x-ai/')) or {}
    except (OSError, ValueError, TypeError):
        return {}


def video_quote(model, duration, images=0, video_seconds=0):
    try:
        return _video_quote(model, duration, images, video_seconds)
    except (ArithmeticError, KeyError, TypeError, ValueError, AttributeError):
        return {'pricingStatus': 'unknown', 'note': '暂无可用价格数据，以供应商实际计费为准。'}


def _video_quote(model, duration, images=0, video_seconds=0):
    p = media_pricing(model)
    if p.get('unit') != 'second' or not p.get('output'):
        return {'pricingStatus': 'unknown', 'note': '暂无价格数据，以供应商实际计费为准。'}
    estimates = {}
    for resolution, amount in p['output'].items():
        rate = Decimal(str(amount))
        if not rate.is_finite() or rate < 0:
            raise ValueError('Invalid catalog rate')
        price = rate * Decimal(str(duration))
        price += Decimal(str(p.get('inputImage', 0))) * max(0, images - p.get('freeInputImages', 0))
        price += (rate if p.get('inputVideoAtOutputRate') else Decimal(str(p.get('inputVideoSecond', 0)))) * Decimal(str(video_seconds))
        estimates[resolution] = format(price, '.2f')
    return {'model': model, 'currency': p['currency'], 'estimated_cost': estimates,
            'pricing_url': p.get('source'), 'pricingStatus': 'reference',
            'note': '模型库中的官方参考价，最终以供应商实际计费为准。'}
