#!/usr/bin/env python3
"""Accept an Agent-reviewed Shopee candidate after strict Excel/page evidence checks."""
import argparse
from pathlib import Path

from report_state import read_json, write_json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--date", required=True)
    args = parser.parse_args()

    capture = read_json(args.capture)
    checkpoint = read_json(args.checkpoint)
    if capture.get("dateIso") != args.date or checkpoint.get("identity", {}).get("dateIso") != args.date:
        raise SystemExit("目标日期与采集文件不一致")
    target = capture.get("businessExport", {}).get("target")
    if not target or target.get("sales") is None or target.get("orders") is None:
        raise SystemExit("经营 Excel 缺少目标日有效行")
    if capture.get("sales") != target.get("sales") or capture.get("orders") != target.get("orders"):
        raise SystemExit("正式候选值与经营 Excel 目标日行不一致")
    if len(capture.get("downloads", [])) < 2:
        raise SystemExit("经营和广告 Excel 归档不完整")
    if capture.get("adSource") == "single-day-page-fallback":
        expected = [f"{args.date}T00:00:00-03:00", f"{args.date}T23:59:59-03:00"]
        if capture.get("adsPage", {}).get("verifiedPeriod") != expected:
            raise SystemExit("广告页面未核验目标单日 GMT-03")
        if capture.get("adCost") is None or capture.get("roi") is None:
            raise SystemExit("广告页面回退值不完整")
    elif capture.get("adCost") is None:
        raise SystemExit("广告数据未取得")
    capture["missingReason"] = ""
    capture["status"] = "collected"
    capture["agentReview"] = "已核验经营 Excel 目标日行、广告 Excel 归档及目标单日 GMT-03 页面回退证据"
    write_json(args.capture, capture)
    checkpoint["status"] = "collected"
    checkpoint["agentReview"] = capture["agentReview"]
    write_json(args.checkpoint, checkpoint)
    print(f"Shopee 候选复核通过: {capture['storeId']} {args.date}")


if __name__ == "__main__":
    main()
