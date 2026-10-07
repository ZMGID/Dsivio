"""Collect target-day GMV Max cost from the store's hourly overview Excel."""
import argparse
from decimal import Decimal, ROUND_HALF_UP
import json
from pathlib import Path
import time

import openpyxl
from download_files import archive, digest
from report_state import read_json, write_json, require
from tiktok_export import number

URL = 'https://seller-br.tiktok.com/ads-creation/dashboard'


def parse(path, day):
    workbook = openpyxl.load_workbook(path, read_only=True, data_only=True)
    try:
        sheet = workbook.active
        sheet.reset_dimensions()
        rows = list(sheet.values)
    finally:
        workbook.close()
    expected = ['时间', '成本', 'SKU 订单数（当前店铺）', '平均下单成本', '总收入（当前店铺）', 'ROI', '币种']
    require(list(rows[0]) == expected, '广告 Excel 表头不匹配，需 Agent 核对实际语言/口径')
    detail = [row for row in rows[1:] if row[0] != '-']
    totals = [row for row in rows[1:] if row[0] == '-']
    require(len(totals) == 1 and len(detail) == 24, '单日广告 Excel 必须包含24小时及唯一总计')
    require({str(r[0]) for r in detail} == {f'{day} {h:02d}:00:00' for h in range(24)}, '广告 Excel 日期错误或小时重复/缺失')
    require(all(r[6] == 'BRL' for r in rows[1:]), '广告币种不是 BRL')
    reconciliation = {}
    for index in (1, 4):
        values = [number(r[index]) for r in detail]
        require(all(v is not None and v >= 0 for v in values), '广告成本/收入缺失或无效')
        total = number(totals[0][index])
        require(total is not None and total >= 0, '广告成本/收入总计缺失或无效')
        difference = sum(values, Decimal(0)) - total
        require(difference == 0 if index == 1 else abs(difference) <= Decimal('.01'), '广告逐小时与总计不一致')
        reconciliation['成本' if index == 1 else '收入'] = {'hourlySum': str(sum(values, Decimal(0))), 'total': str(total), 'difference': str(difference)}
    cost, revenue, roi = (number(totals[0][i]) for i in (1, 4, 5))
    require(roi is not None and roi >= 0, '广告 ROI 缺失')
    if cost:
        require((revenue / cost).quantize(Decimal('.01'), rounding=ROUND_HALF_UP) == roi, '广告 ROI 与总收入/成本不一致')
    return {'adCost': str(cost), 'roi': str(roi), 'adRevenue': str(revenue),
            'adSkuOrders': int(totals[0][2]), 'hourlyRows': detail, 'adReconciliation': reconciliation}


def collect(args, capture_file, source=None):
    from collect_tiktok import cli, files, wait_file, HERE, now
    capture_file = Path(capture_file)
    capture = read_json(capture_file)
    checkpoint = capture_file.with_name('ads-checkpoint.json')
    state = read_json(checkpoint) if checkpoint.exists() else {'dateIso': args.date, 'account': args.expected_account, 'stage': 'new'}
    require(state['dateIso'] == args.date and state['account'] == args.expected_account, '广告断点身份/日期不一致')
    download = Path(args.download_dir).resolve(strict=True)
    if state.get('archive'):
        entry = state['archive']
        require(digest(Path(entry['path'])) == entry['sha256'], '广告归档哈希发生变化')
        source = Path(entry['path'])
    elif source is None:
        if state['stage'] == 'new':
            cli('page', 'visit', '--store-id', args.store_id, '--url', URL, structured=False)
            deadline = time.monotonic() + args.ready_wait
            while True:
                try:
                    target = cli('page', 'content', '--store-id', args.store_id)['targetId']
                    reply = cli('page', 'exec', '--store-id', args.store_id, '--target-id', target, '--script-file', HERE/'tiktok_ads_observe.js')
                    page = json.loads(reply['result'])
                    require(page['url'] == URL and args.expected_account in [x.strip() for x in page['text'].splitlines()], '广告账号/页面尚未就绪')
                    require('(UTC-03:00)' in page['text'] and 'BRL' in page['text'], '广告时区/币种未确认')
                    require(page.get('start') and page.get('end'), '广告日期未就绪')
                    break
                except (ValueError, RuntimeError, KeyError):
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(3)
            if page['start'] != args.date or page['end'] != args.date:
                cli('page', 'click', '--store-id', args.store_id, '--selector', 'input[placeholder="开始日期"]', structured=False)
                page = json.loads(cli('page', 'exec', '--store-id', args.store_id, '--target-id', target, '--script-file', HERE/'tiktok_ads_observe.js')['result'])
                require(page['yesterdaySelector'] is not None, '广告昨天控件不唯一，需 Agent 选择目标日')
                cli('page', 'click', '--store-id', args.store_id, '--selector', page['yesterdaySelector'], structured=False)
                time.sleep(3)
                page = json.loads(cli('page', 'exec', '--store-id', args.store_id, '--target-id', target, '--script-file', HERE/'tiktok_ads_observe.js')['result'])
            require(page['start'] == page['end'] == args.date, '广告日期不是冻结目标日，需 Agent 选择准确日期')
            require(page['exportSelector'], '广告导出按钮不唯一')
            state.update(stage='waiting_export', baseline=files(download), page={k:page[k] for k in ('url','start','end')}, timeZone='UTC-03:00', submittedAt=now())
            write_json(checkpoint, state)
            cli('page', 'click', '--store-id', args.store_id, '--selector', page['exportSelector'], structured=False)
        source = wait_file(download, state['baseline'], args.max_wait)
        require(source is not None, '原广告导出尚未落盘，需继续等待同次任务')
    parsed = parse(source, args.date)
    if not state.get('archive'):
        entry = archive(capture_file, args.store_id, source, download, '广告组数据', args.date, args.date)
        state.update(stage='collected', archive=entry)
        write_json(checkpoint, state)
    capture = read_json(capture_file)
    capture.update(**parsed, adSource='gmv-max-hourly-excel', adMissingReason='',
                   adEvidence={'dateIso':args.date,'account':args.expected_account,'timeZone':'UTC-03:00',
                               'currency':'BRL','archive':entry,'metric':'成本（非预算/净成本/充值）'}, status='collected')
    write_json(capture_file, capture)
    return capture


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('store-id', 'expected-account', 'date', 'download-dir', 'capture'):
        parser.add_argument('--'+key, required=True)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--ready-wait', type=int, default=90)
    parser.add_argument('--max-wait', type=int, default=600)
    args = parser.parse_args()
    result = collect(args, args.capture, args.source)
    print(json.dumps({k:result[k] for k in ('dateIso','adCost','roi')}, ensure_ascii=False))
