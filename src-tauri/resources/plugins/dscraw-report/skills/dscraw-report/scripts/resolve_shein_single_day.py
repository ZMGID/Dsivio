#!/usr/bin/env python3
"""Resolve a SHEIN period-total discrepancy with Agent-verified target-day page totals."""
import argparse
from decimal import Decimal
from pathlib import Path

from report_state import read_json, write_json
from shein_export import METRICS, parse_export, reconcile
from collect_tiktok import serializable


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--date", required=True)
    parser.add_argument("--gmv", required=True)
    parser.add_argument("--units", required=True)
    parser.add_argument("--paid-orders", required=True)
    args = parser.parse_args()

    capture = read_json(args.capture)
    checkpoint = read_json(args.checkpoint)
    if capture.get("dateIso") != args.date or checkpoint.get("identity", {}).get("dateIso") != args.date:
        raise SystemExit("目标日期与采集文件不一致")
    saved_exports = {metric: capture["exports"][metric] for metric in METRICS}
    exports = {
        metric: parse_export(saved_exports[metric]["archive"]["path"], metric,
                             saved_exports[metric]["start"], saved_exports[metric]["end"], args.date)
        for metric in METRICS
    }
    single = {"GMV": Decimal(args.gmv), "销量": Decimal(args.units), "支付订单数": Decimal(args.paid_orders)}
    for metric in METRICS:
        target = next((row.get(metric) for row in exports[metric]["dailyRecords"] if row.get("dateIso") == args.date), None)
        if target is None or Decimal(str(target)) != single[metric]:
            raise SystemExit(f"{metric} 的单日页面值与 Excel 目标日行不一致")
    result = reconcile(exports, {metric: saved_exports[metric]["page"]["totals"][metric] for metric in METRICS},
                       capture["orderMetric"], single)
    if result["missingReason"]:
        raise SystemExit(result["missingReason"])
    capture.update(serializable(result))
    capture["singleDayPage"] = {"dateIso": args.date, "totals": {key: str(value) for key, value in single.items()},
                                "purpose": "Agent 核验 Excel 目标日行；未替代 Excel 数据源"}
    capture["status"] = "collected"
    write_json(args.capture, capture)
    checkpoint["status"] = "collected"
    checkpoint["singleDayPage"] = capture["singleDayPage"]
    write_json(args.checkpoint, checkpoint)
    print(f"SHEIN 单日复核通过: {args.date}")


if __name__ == "__main__":
    main()
