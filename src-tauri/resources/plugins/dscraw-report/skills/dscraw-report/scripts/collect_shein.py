"""Agent-supervised SHEIN BR collection: three fixed chart exports."""
import argparse
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
import json
import os
import re
import time

# Reuse the tested CLI/transfer utilities; do not run TikTok's page workflow.
from collect_tiktok import cli, files, wait_file, now, log, serializable
from download_files import archive, digest
from report_state import read_json, write_json
from shein_export import METRICS, number, parse_export, reconcile
from agent_handoff import handoff

HERE = Path(__file__).resolve().parent
URL = 'https://sellerhub.shein.com/#/sbn/managementAnalysis/trade'


def observe(store):
    target = cli('page', 'content', '--store-id', store)['targetId']
    result = cli('page', 'exec', '--store-id', store, '--target-id', target,
                 '--script-file', HERE / 'shein_observe.js')
    if result.get('exceptionDetails'):
        raise ValueError('页面观察脚本异常')
    state = json.loads(result['result'])
    state['_target'] = target
    return state


def click_label(store, label):
    state = observe(store)  # Resolve positional selectors afresh, never persist them.
    candidates = [item for item in state['controls'] if item['text'] == label]
    if len(candidates) != 1:
        raise ValueError(f'可见控件不唯一或不可用: {label}')
    cli('page', 'click', '--store-id', store, '--target-id', state['_target'],
        '--selector', candidates[0]['selector'], structured=False)
    log('已点击 '+label)


def period(state):
    match = re.fullmatch(r'(\d{4}/\d{2}/\d{2})(?:\s*-\s*(\d{4}/\d{2}/\d{2}))?', state.get('period') or '')
    if not match:
        raise ValueError('统计期间尚未就绪或格式不明确')
    start, end = match[1], match[2] or match[1]
    return date.fromisoformat(start.replace('/', '-')).isoformat(), date.fromisoformat(end.replace('/', '-')).isoformat()


def ready(args, expected=None, chart=None):
    until, previous = time.monotonic()+args.ready_wait, None
    reason = '页面尚未就绪'
    while time.monotonic() < until:
        state = observe(args.store_id)
        if 'ERR_SOCKS_CONNECTION_FAILED' in state['text']:
            raise ValueError('代理错误，交回 Agent 处理')
        try:
            if args.expected_account not in [line.strip() for line in state['text'].splitlines()]:
                raise ValueError('页面账号未匹配；可能尚在登录或验证')
            if state['url'] != URL:
                raise ValueError('尚未进入交易概览')
            dates = period(state)
            if expected and dates != expected:
                raise ValueError('实际统计期间与目标范围不一致')
            if not state.get('updatedAt') or not state.get('site') or not (state['totals']['GMV'] or '').startswith('BRL '):
                raise ValueError('更新时间、站点或 BRL 币种缺失')
            if any(number(value) is None for value in state['totals'].values()):
                raise ValueError('指标尚未就绪')
            if chart and state['chart'] != chart+'趋势图':
                raise ValueError('图表类型尚未切换完成')
            signature = (dates, state['chart'], tuple(state['totals'].items()))
            if signature == previous:
                return state
            previous = signature
        except ValueError as error:
            reason, previous = str(error), None
        time.sleep(3)
    raise ValueError(reason)


def evidence(state):
    return {key: state[key] for key in ('url', 'period', 'updatedAt', 'site', 'totals', 'chart')}


def run(args):
    root = Path(args.output_root).resolve()
    folder = root / '数据' / args.date / 'SHEIN采集' / args.store_id
    folder.mkdir(parents=True, exist_ok=True)
    checkpoint, capture = folder/'checkpoint.json', folder/'capture.json'
    lock = folder/'running.lock'
    with lock.open('x', encoding='utf-8') as handle:
        handle.write(str(os.getpid()))
    state = {}
    identity = {'storeId': args.store_id, 'name': args.expected_name, 'account': args.expected_account,
                'dateIso': args.date, 'orderMetric': args.order_metric}
    try:
        state = read_json(checkpoint) if checkpoint.exists() else {'identity': identity, 'items': {}}
        if state['identity'] != identity:
            raise ValueError('断点身份、目标日或数量口径与参数不一致')
        if state.get('status') in ('success', 'collected'):
            result = read_json(capture)
            for item in result['exports'].values():
                if digest(Path(item['archive']['path'])) != item['archive']['sha256']:
                    raise ValueError('已完成任务的归档文件缺失或改变')
            log('复用已核验三份文件，不开店、不重新导出')
            return result
        download = Path(args.download_dir).resolve(strict=True)
        if state.get('downloadDir', str(download)) != str(download):
            raise ValueError('店铺下载目录发生变化')
        state['downloadDir'] = str(download)
        ready(args)
        click_label(args.store_id, '近7天')
        # The shortcut is not enough: validate actual dates after it settles.
        page = ready(args)
        start, end = period(page)
        if (date.fromisoformat(end)-date.fromisoformat(start)).days != 6 or not start <= args.date <= end:
            raise ValueError('当前近七天窗口不覆盖目标日')
        if state.get('range', [start, end]) != [start, end]:
            raise ValueError('恢复窗口已改变，不能以新窗口补旧任务')
        state['range'] = [start, end]
        write_json(checkpoint, state)
        if not capture.exists():
            write_json(capture, {'dateIso': args.date, 'configSnapshot': {'report': {'outputRoot': str(root)},
                        'stores': [{'storeId': args.store_id}]}, 'downloads': []})
        parsed, totals = {}, {}
        for metric in METRICS:
            item = state['items'].setdefault(metric, {})
            # Archive manifests recover the copy/cleanup checkpoint gap.
            archived = [read_json(path) for path in read_json(capture).get('downloads', [])
                        if read_json(path)['type'] == metric]
            if len(archived) > 1:
                raise ValueError('同一指标存在多份归档，需核验来源')
            if archived:
                item['archive'] = archived[0]
            if not item.get('archive'):
                if not item.get('submittedAt'):
                    click_label(args.store_id, '新客'+metric)
                    selected = ready(args, (start, end), metric)
                    item.update(submittedAt=now(), baseline=files(download), page=evidence(selected))
                    write_json(checkpoint, state)  # Durable intent BEFORE export click.
                    click_label(args.store_id, '导出')
                source = wait_file(download, item['baseline'], args.max_wait)
                if source is None:
                    raise TimeoutError(metric+'导出已提交，尚未落盘；续跑只等待同一份文件')
                parse_export(source, metric, start, end, args.date)
                item['archive'] = archive(capture, args.store_id, source, download, metric, start, end)
                write_json(checkpoint, state)
            entry = item['archive']
            if digest(Path(entry['path'])) != entry['sha256']:
                raise ValueError('归档哈希不一致')
            parsed[metric] = parse_export(entry['path'], metric, start, end, args.date)
            totals[metric] = item['page']['totals'][metric]
            log(metric+'文件已归档和解析')
        single = state.get('singleDayVerification')
        if single and (single.get('dateIso') != args.date or single.get('account') != args.expected_account):
            raise ValueError('Agent 单日复核的日期或账号不匹配')
        result = reconcile(parsed, totals, args.order_metric, single.get('totals') if single else None)
        result.update(identity)
        result.update(exports={key: {**parsed[key], 'archive': state['items'][key]['archive'],
                                     'page': state['items'][key]['page']} for key in METRICS},
                      capturedAt=now(),
                      timeZone='America/Sao_Paulo', timeZoneSource='BR 站点配置，非页面检测',
                      status='needs_agent' if result['missingReason'] else 'collected')
        result = {**read_json(capture), **serializable(result)}
        write_json(capture, result)
        state['status'] = result['status']
        write_json(checkpoint, state)
        write_json(folder/'handoff.json', {**identity, 'status': result['status'], 'reason': 'Agent 核对原始 exports；有差异由 Agent 单日复核，决定采用及关店'})
        return result
    except Exception as error:
        return handoff(folder, identity, error)
    finally:
        lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--store-id', required=True)
    parser.add_argument('--expected-name', required=True)
    parser.add_argument('--expected-account', required=True)
    parser.add_argument('--order-metric', required=True, choices=['unitsSold', 'paidOrders'])
    parser.add_argument('--date', default=(datetime.now(timezone(timedelta(hours=-3))).date()-timedelta(days=1)).isoformat())
    parser.add_argument('--output-root', required=True)
    parser.add_argument('--ready-wait', type=int, default=90)
    parser.add_argument('--max-wait', type=int, default=60)
    parser.add_argument('--download-dir', required=True, help='Agent 从本店开店返回值获取的下载目录')
    args = parser.parse_args()
    date.fromisoformat(args.date)
    if not args.store_id.isdigit() or args.ready_wait < 10 or args.max_wait < 10 or not args.expected_account.strip():
        parser.error('店铺 ID、账号或等待参数无效')
    result = run(args)
    log(json.dumps({key: result.get(key) for key in ('status','storeId','dateIso','reason','missingReason')}, ensure_ascii=False))
    return 0 if result['status'] in ('success', 'collected') else 2


if __name__ == '__main__':
    raise SystemExit(main())
