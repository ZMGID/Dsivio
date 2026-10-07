#!/usr/bin/env python3
import argparse
import json
import subprocess
from datetime import datetime
from decimal import Decimal
from html import escape
from pathlib import Path


def load(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def number(value):
    return Decimal(str(value)) if value not in (None, "") else Decimal("0")


def successful(store):
    return store.get("sales") is not None


def category(source, category_id, name):
    report = load(source)
    stores = report["stores"]
    captured = [store for store in stores if successful(store)]
    result = {
        "id": category_id,
        "name": name,
        "storeCount": len(stores),
        "capturedStoreCount": len(captured),
        "sales": f'{sum((number(store.get("sales")) for store in captured), Decimal("0")):.2f}',
        "status": "成功" if len(captured) == len(stores) else "部分完成",
        "source": str(source),
    }
    if category_id == "kidswear":
        order_count = sum(int(store.get("orders") or 0) for store in captured if store.get("platform") in {"shopee", "tiktok"})
        tiktok_units = sum(int(store.get("itemsSold") or 0) for store in captured if store.get("platform") == "tiktok")
        shein_units = sum(int(store.get("unitsSold") or store.get("orders") or 0) for store in captured if store.get("platform") == "shein")
        shein_paid = sum(int(store.get("paidOrders") or 0) for store in captured if store.get("platform") == "shein")
        result.update({
            "orderCount": order_count,
            "tiktokUnits": tiktok_units,
            "sheinUnits": shein_units,
            "sheinPaidOrders": shein_paid,
            "quantityDisplay": f"Shopee/TikTok订单 {order_count} · TikTok商品件数 {tiktok_units} · SHEIN销量 {shein_units}（支付订单 {shein_paid}）",
        })
    else:
        result.update({
            "quantityLabel": "订单数",
            "quantity": sum(int(store.get("orders") or 0) for store in captured),
        })
    return report, result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("date")
    parser.add_argument("kidswear", type=Path)
    parser.add_argument("lighting", type=Path)
    parser.add_argument("luggage", type=Path)
    parser.add_argument("output_root", type=Path)
    args = parser.parse_args()

    reports = []
    categories = []
    for source, category_id, name in [
        (args.kidswear, "kidswear", "童装"),
        (args.lighting, "lighting", "灯具"),
        (args.luggage, "luggage", "箱包"),
    ]:
        report, item = category(source.resolve(), category_id, name)
        if report.get("dateIso") != args.date:
            raise SystemExit(f"日期不一致: {source}={report.get('dateIso')}，目标={args.date}")
        reports.append(report)
        categories.append(item)

    stores = [store for report in reports for store in report["stores"]]
    captured = [store for store in stores if successful(store)]
    sales_total = sum((number(item["sales"]) for item in categories), Decimal("0"))
    ad_total = sum((number(store.get("adCost")) for store in stores if store.get("adCost") is not None), Decimal("0"))
    payload = {
        "dateIso": args.date,
        "currency": "BRL",
        "categoryCount": 3,
        "storeCount": len(stores),
        "capturedStoreCount": len(captured),
        "salesTotal": f"{sales_total:.2f}",
        "adCostTotal": f"{ad_total:.2f}",
        "status": "成功" if len(captured) == len(stores) else "部分完成",
        "categories": categories,
        "quantityNote": "数量口径不合并：童装分别显示 Shopee/TikTok 订单数、TikTok 商品成交件数、SHEIN 销量和支付订单数；灯具和箱包分别显示订单数。",
        "coverageNote": f"已获取销售额覆盖 {len(captured)}/{len(stores)} 家店。",
        "adCostNote": "广告费汇总童装 Shopee 与已开启广告采集的 TikTok 已核验成本，ROI不合并。",
        "generatedAt": datetime.now().astimezone().isoformat(timespec="seconds"),
    }

    data_dir = args.output_root / "数据" / args.date
    report_dir = args.output_root / "日报" / args.date
    data_dir.mkdir(parents=True, exist_ok=True)
    report_dir.mkdir(parents=True, exist_ok=True)
    json_path = data_dir / f"三类日报汇总_{args.date}.json"
    html_path = report_dir / f"三类日报汇总_{args.date}.html"
    png_path = report_dir / f"三类日报汇总_{args.date}.png"
    json_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    cards = []
    for item in categories:
        quantity = item.get("quantityDisplay") or f'{item["quantityLabel"]} {item["quantity"]}'
        badge_class = "partial" if item["status"] != "成功" else ""
        cards.append(f'''<article class="card"><div class="card-head"><h2>{escape(item["name"])}</h2><span class="{badge_class}">{item["capturedStoreCount"]}/{item["storeCount"]} 家</span></div><div class="sales"><small>销售额（BRL）</small><strong>{Decimal(item["sales"]):,.2f}</strong></div><p>{escape(quantity)}</p></article>''')
    html = f'''<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>三类日报汇总 {args.date}</title>
<style>*{{box-sizing:border-box}}body{{margin:0;background:#f5f7fa;color:#152b45;font-family:"Microsoft YaHei",Arial,sans-serif}}.hero{{background:#173c68;color:white;padding:28px 48px;border-bottom:4px solid #42a5df}}.hero h1{{margin:0 0 12px;font-size:30px}}.meta{{display:flex;gap:28px;font-size:15px}}main{{padding:26px 44px 34px}}.total{{display:flex;align-items:end;justify-content:space-between;background:white;border:1px solid #dce4ec;border-radius:8px;padding:20px 24px;margin-bottom:20px}}.total small,.sales small{{display:block;color:#667b91;font-weight:700;margin-bottom:7px}}.total strong{{font-size:40px}}.coverage{{font-weight:700;color:#9a5b12}}.grid{{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}}.card{{background:white;border:1px solid #dce4ec;border-radius:8px;padding:20px;min-height:190px}}.card-head{{display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid #dce4ec;padding-bottom:12px}}h2{{margin:0;font-size:22px}}.card-head span{{background:#eaf5ef;color:#24734e;padding:5px 10px;border-radius:14px;font-weight:700}}.card-head span.partial{{background:#fff2df;color:#9a5b12}}.sales{{padding:20px 0 14px}}.sales strong{{font-size:32px}}.card p{{margin:0;color:#415a73;line-height:1.65}}.note{{margin-top:20px;background:#edf3f8;border-left:4px solid #42a5df;padding:14px 16px;line-height:1.7;color:#415a73}}footer{{padding:0 44px 26px;color:#7b8c9d;font-size:13px}}</style></head>
<body><header class="hero"><h1>三类日报汇总 · {args.date}</h1><div class="meta"><span>品类 3</span><span>店铺 {len(captured)}/{len(stores)}</span><span>币种 BRL</span></div></header><main><section class="total"><div><small>三类销售额合计（BRL）</small><strong>{sales_total:,.2f}</strong></div><div class="coverage">{escape(payload["coverageNote"])}</div></section><section class="grid">{''.join(cards)}</section><div class="note">{escape(payload["quantityNote"])}<br>{escape(payload["adCostNote"])}</div></main><footer>生成时间：{escape(payload["generatedAt"])}</footer></body></html>'''
    html_path.write_text(html, encoding="utf-8")
    screenshot = Path(__file__).with_name("screenshot_html.cjs")
    subprocess.run(["node", str(screenshot), str(html_path), str(png_path)], check=True)
    print(json.dumps({"json": str(json_path), "html": str(html_path), "png": str(png_path), "payload": payload}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
