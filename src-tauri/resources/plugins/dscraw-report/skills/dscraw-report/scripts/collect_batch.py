"""Four concurrent collectors, six owned windows; recovery belongs to the Agent."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from datetime import date
from functools import partial
import json
import os
from pathlib import Path
from queue import Queue
import time

import collect_tiktok as tiktok
import collect_shein as shein
import collect_shopee as shopee
from report_state import load_snapshot, target_date, read_json, write_json, require

PLATFORMS = {'tiktok': (tiktok.URL, 'TK采集', tiktok.collect),
             'shein': (shein.URL, 'SHEIN采集', shein.run),
             'shopee': (shopee.BUSINESS, 'Shopee采集', shopee.run)}
WAITING = ('awaiting_agent', 'needs_agent')


def new_batch(snapshot, day):
    date.fromisoformat(day)
    require(all(s['platform'] in PLATFORMS for s in snapshot['stores']), '平台未适配')
    return {'dateIso': day, 'configSnapshot': snapshot, 'stores': {
        s['storeId']: {'store': copy.deepcopy(s), 'status': 'queued', 'window': False, 'revision': 0}
        for s in snapshot['stores']}}


def change(item, status, reason=''):
    item.update(status=status, reason=reason, revision=item['revision'] + 1)


def save(path, state):
    state['active'] = sum(s['status'] == 'collecting' for s in state['stores'].values())
    state['windows'] = sum(s['window'] for s in state['stores'].values())
    state['status'] = ('collected' if all(s['status'] == 'done' for s in state['stores'].values())
                       else 'running' if state['active'] else 'needs_agent')
    write_json(path, state)


def open_store(item):
    store = item['store']
    browser = tiktok.cli('store', 'open', '--id', store['storeId'],
                        '--expected-name', store['expectedName'], '--url', PLATFORMS[store['platform']][0])
    require(str(browser.get('storeId')) == store['storeId'] and browser.get('name') == store['expectedName'],
            '开店返回身份不一致')
    return str(Path(browser['downloadFolderPath']).resolve(strict=True))


def check_startup(args, platform):
    """Read-only normal-page check; never click login, verification or navigation."""
    require(args.expected_account.strip(), '页面账号未配置，交给 Agent 现场核对')
    deadline = time.monotonic() + 30
    while True:
        try:
            if platform == 'tiktok':
                target = tiktok.cli('page', 'content', '--store-id', args.store_id)['targetId']
                page = tiktok.observe(args.store_id, target)
                tiktok.verify_page(page, args.expected_account)
            elif platform == 'shein':
                page = shein.observe(args.store_id)
                require(page['url'] == shein.URL, '启动页面不是交易概览，需 Agent 处理')
                require(args.expected_account in [x.strip() for x in page['text'].splitlines()], '页面账号未匹配')
                shein.period(page)
                require(page.get('updatedAt') and page.get('site') and
                        (page['totals']['GMV'] or '').startswith('BRL '), '启动页面字段尚未就绪')
                require(all(shein.number(v) is not None for v in page['totals'].values()), '指标尚未就绪')
            else:
                page = shopee.observe(args.store_id)
                if page['proxyError'] or any(x['key'] in ('login', 'verify') for x in page['controls']):
                    raise RuntimeError('启动页面有登录、验证或代理错误，交给 Agent')
                require(page['url'] == shopee.BUSINESS and not page['loading'], '经营页面尚未就绪')
                require(args.expected_account in [x.strip() for x in page['identityLines']], '页面账号未匹配')
                require('订单类型\n已下订单' in page['business'], '订单口径未就绪')
                shopee.dates(page)
            return
        except ValueError as error:
            if time.monotonic() >= deadline:
                raise ValueError('启动检查未通过：'+str(error)) from error
            time.sleep(3)


def collect_store(item, snapshot, day, on_started=None):
    store = item['store']
    root = snapshot['report']['outputRoot']
    capture = Path(root)/'数据'/day/PLATFORMS[store['platform']][1]/store['storeId']/'capture.json'
    # A failed close can be retried without rerunning the collector.
    result = item.get('result', {})
    if result.get('status') not in ('success', 'collected'):
        args = argparse.Namespace(store_id=store['storeId'], expected_name=store['expectedName'],
                                  expected_account=store.get('expectedAccount', ''), date=day,
                                  download_dir=item['downloadDir'], output_root=root,
                                  collect_ads=store['collectAds'], order_metric=store['orderMetric'],
                                  ready_wait=90, max_wait=600 if store['platform'] == 'tiktok' else 60)
        if item.get('startup') != 'agent_ready':
            check_startup(args, store['platform'])
        if on_started:
            on_started()
        result = PLATFORMS[store['platform']][2](args)
    summary = {'status': result['status'], 'capture': str(capture),
               'reason': result.get('reason') or result.get('missingReason') or '候选结果待 Agent 核验'}
    if result['status'] not in ('success', 'collected'):
        return {'status': 'needs_agent', 'result': summary, 'reason': summary['reason']}
    try:
        tiktok.cli('store', 'close', '--id', store['storeId'], structured=False)
    except Exception as error:
        return {'status': 'needs_agent', 'result': summary, 'reason': '关店未确认：'+str(error)}
    return {'status': 'done', 'result': summary, 'window': False, 'reason': ''}


def request(path, ids, action, account=None, download_dir=None):
    state = read_json(path)
    require(not account or len(ids) == 1, '账号只能用于一家店')
    require(not download_dir or len(ids) == 1, '下载目录只能用于一家店')
    for store_id in ids:
        require(store_id in state['stores'], '店铺不在本批次中')
        item = state['stores'][store_id]
        require(item['status'] in WAITING, '只能处理已交给 Agent 的店，不能抢占运行中的店')
        write_json(path.parent/'commands'/f'{store_id}.json', {
            'revision': item['revision'], 'action': action,
            'account': account, 'downloadDir': download_dir})


def commands(path, state):
    for store_id, item in state['stores'].items():
        file = path.parent/'commands'/f'{store_id}.json'
        if not file.exists():
            continue
        command = read_json(file)
        file.unlink()  # Only consume this tool's one-store request, not task evidence.
        if item['status'] not in WAITING or command['revision'] != item['revision']:
            print(f'{store_id}: 忽略过期请求，不抢占店铺', flush=True)
            continue
        if command['action'] == 'ready':
            if command.get('account'):
                item['store']['expectedAccount'] = command['account']
            if command.get('downloadDir'):
                item['downloadDir'] = str(Path(command['downloadDir']).resolve())
            if not item.get('downloadDir') or not item['store'].get('expectedAccount', '').strip():
                change(item, 'needs_agent', 'Agent 需提供本店页面账号及开店返回的下载目录')
            else:
                item.pop('openError', None)
                item['startup'] = 'agent_ready'
                change(item, 'ready')
        elif command['action'] in ('close', 'defer'):
            try:
                tiktok.cli('store', 'close', '--id', store_id, structured=False)
                item['window'] = False
                item.pop('openError', None)
                if command['action'] == 'defer':
                    item['deferredReason'] = item.get('reason', '')
                    if item.get('result', {}).get('status') in ('success', 'collected'):
                        change(item, 'done')  # Only closing had failed; no data retry needed.
                    else:
                        change(item, 'deferred', '暂缓，正常队列结束后由 Agent 发起补采')
                else:
                    change(item, 'closed', 'Agent 已处理并要求关店；最终日报结果由 Agent 汇总')
            except Exception as error:
                change(item, 'needs_agent', '关店未确认：'+str(error))
        save(path, state)


def run(path, retry_deferred=False, retry_deferred_after=0):
    path = Path(path).resolve()
    lock = path.with_suffix('.lock')
    with lock.open('x', encoding='utf-8') as handle:
        handle.write(str(os.getpid()))
    try:
        state = read_json(path)
        if retry_deferred:
            require(not any(s['status'] in ('queued', 'opening', 'ready', 'collecting') or s.get('openError')
                            for s in state['stores'].values()), '先完成正常队列并处理全局开店故障，再补采暂缓店')
            require(not any(s['status'] in WAITING for s in state['stores'].values()),
                    '先把重复失败店处理为 deferred，再统一延迟补采')
            deferred = [s for s in state['stores'].values() if s['status'] == 'deferred']
            require(deferred, '本批次没有待补采的 deferred 店铺')
            if retry_deferred_after:
                schedule = state.get('deferredRetry')
                if not schedule:
                    now = time.time()
                    schedule = {'scheduledAtEpoch': now,
                                'notBeforeEpoch': now + retry_deferred_after,
                                'delaySeconds': retry_deferred_after}
                    state['deferredRetry'] = schedule
                    save(path, state)
                    print(f'deferred 店铺将在 {retry_deferred_after} 秒后重新启动', flush=True)
                remaining = max(0, schedule['notBeforeEpoch'] - time.time())
                if remaining:
                    time.sleep(remaining)
                state.pop('deferredRetry', None)
            for item in state['stores'].values():
                if item['status'] == 'deferred':
                    item.pop('startup', None)  # Reopened stores need a fresh startup check.
                    change(item, 'queued', '等待后重新启动，仅补采暂缓店')
        # A crash never grants permission to resume a store automatically.
        for item in state['stores'].values():
            if item['status'] in ('opening', 'collecting'):
                change(item, 'needs_agent', '上次调度中断；Agent 核对原进程和本店现场后再 ready')
            elif item['status'] == 'awaiting_agent' and item.get('downloadDir'):
                change(item, 'ready', '旧版待观察店转为脚本启动检查')
        save(path, state)
        pending = {}
        opening = None
        started = Queue()
        with ThreadPoolExecutor(max_workers=4) as pool, ThreadPoolExecutor(max_workers=1) as opener:
            while True:
                if opening and opening[0].done():
                    future, store_id = opening
                    item = state['stores'][store_id]
                    try:
                        item['downloadDir'] = future.result()
                        change(item, 'ready', '已开店，自动检查后采集')
                    except Exception as error:
                        item['openError'] = True
                        change(item, 'needs_agent', str(error))
                    opening = None
                    save(path, state)
                    print(f"{store_id}: {item['status']} {item['reason']}", flush=True)
                for future, store_id in list(pending.items()):
                    if not future.done():
                        continue
                    item = state['stores'][store_id]
                    try:
                        outcome = future.result()
                        item.update({k: v for k, v in outcome.items() if k not in ('status', 'reason')})
                        change(item, outcome['status'], outcome.get('reason', ''))
                    except Exception as error:
                        change(item, 'needs_agent', str(error))
                    del pending[future]
                    save(path, state)
                    print(f"{store_id}: {item['status']} {item['reason']}", flush=True)
                while not started.empty():
                    store_id = started.get_nowait()
                    state['stores'][store_id]['startup'] = 'normal'
                    save(path, state)
                    print(f'{store_id}: 正常启动，自动采集', flush=True)
                commands(path, state)
                # An open error may be global auth/Bridge failure; don't fan it out.
                open_blocked = any(s.get('openError') for s in state['stores'].values())
                for store_id, item in state['stores'].items():
                    if open_blocked or len(pending) >= 4:
                        break
                    if item['status'] == 'ready':
                        change(item, 'collecting')
                        save(path, state)  # Durable ownership before worker starts.
                        pending[pool.submit(collect_store, copy.deepcopy(item),
                                            state['configSnapshot'], state['dateIso'],
                                            partial(started.put, store_id))] = store_id
                # One opening at a time, while the four collectors keep running.
                candidate = next((s for s in state['stores'].values() if s['status'] == 'queued'), None)
                if candidate and not opening and not open_blocked and sum(s['window'] for s in state['stores'].values()) < 6:
                    candidate['window'] = True  # Reserve even if the response is ambiguous.
                    change(candidate, 'opening')
                    save(path, state)
                    opening = (opener.submit(open_store, copy.deepcopy(candidate)), candidate['store']['storeId'])
                    continue
                if not pending and not opening and (open_blocked or not any(s['status'] == 'ready' for s in state['stores'].values())):
                    save(path, state)
                    return state
                time.sleep(0.2)
    finally:
        lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['run', 'ready', 'close', 'defer', 'status'])
    parser.add_argument('--retry-deferred', action='store_true')
    parser.add_argument('--retry-deferred-after', type=int, default=0, metavar='SECONDS')
    parser.add_argument('--batch', type=Path)
    parser.add_argument('--config')
    parser.add_argument('--report')
    parser.add_argument('--date')
    parser.add_argument('--store-id', nargs='+')
    parser.add_argument('--account')
    parser.add_argument('--download-dir')
    args = parser.parse_args()
    require(not args.retry_deferred or (args.action == 'run' and args.batch), '--retry-deferred 仅用于 run --batch 原批次')
    require(not args.retry_deferred_after or args.retry_deferred,
            '--retry-deferred-after 必须与 --retry-deferred 一起使用')
    require(args.retry_deferred_after >= 0, '--retry-deferred-after 不能为负数')
    path = args.batch
    if path is None:
        require(args.action == 'run', '该命令须指定 --batch')
        snapshot = load_snapshot(args.config, args.report)
        day = target_date(snapshot, args.date)
        path = Path(snapshot['report']['outputRoot'])/'数据'/day/'批量采集'/'batch.json'
        if path.exists():
            require(read_json(path)['configSnapshot'] == snapshot, '已有批次配置不同，请用 --batch 续跑原快照')
        else:
            write_json(path, new_batch(snapshot, day))
    else:
        require(not any((args.config, args.report, args.date)), '--batch 续跑使用原日期和配置，不混传')
    path = path.resolve()
    if args.action in ('ready', 'close', 'defer'):
        require(args.store_id, '请指定 --store-id')
        request(path, args.store_id, args.action, args.account, args.download_dir)
        print('Agent 操作请求已写入；调度运行中会读取，否则使用 run --batch 续跑')
        return 0
    result = run(path, args.retry_deferred, args.retry_deferred_after) if args.action == 'run' else read_json(path)
    print(json.dumps({'batch': str(path), 'dateIso': result['dateIso'],
                      'status': result['status'], 'active': result['active'], 'windows': result['windows'],
                      'stores': {sid: {'name': item['store']['expectedName'],
                                      **{k: item[k] for k in ('status', 'startup', 'reason', 'deferredReason', 'downloadDir', 'result') if k in item}}
                                 for sid, item in result['stores'].items()}}, ensure_ascii=False, indent=2))
    return 0 if result.get('status') == 'collected' else 2


if __name__ == '__main__':
    raise SystemExit(main())
