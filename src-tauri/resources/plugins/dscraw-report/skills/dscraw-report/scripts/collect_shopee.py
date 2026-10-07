"""Single-store Shopee BR collection. No formal ledger/report writes."""
import argparse
from datetime import date, datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from collect_tiktok import cli, now, log, serializable
from download_files import archive, digest
from report_state import read_json, write_json
from shopee_export import number, day, parse_business, parse_ads, reconcile
from agent_handoff import handoff

HERE = Path(__file__).resolve().parent
BUSINESS = 'https://seller.shopee.com.br/datacenter/overview'
ADS = 'https://seller.shopee.com.br/portal/marketing/pas/index'
BR = timezone(timedelta(hours=-3))


def visit(store, url):
    target = cli('page', 'content', '--store-id', store)['targetId']
    reply = subprocess.run([sys.executable, str(HERE/'ziniao_cli.py'), 'page', 'visit',
                            '--store-id', store, '--target-id', target, '--url', url],
                           capture_output=True, encoding='utf-8', timeout=60,
                           env=dict(os.environ, PYTHONIOENCODING='utf-8'))
    if reply.returncode or '页面已导航' not in reply.stdout+reply.stderr:
        raise ValueError('页面导航未确认')


def observe(store):
    # Only retain the target; page content may include embedded script contents.
    target = cli('page', 'content', '--store-id', store)['targetId']
    reply = cli('page', 'exec', '--store-id', store, '--target-id', target,
                '--script-file', HERE/'shopee_observe.js')
    if reply.get('exceptionDetails'):
        raise ValueError('页面观察异常')
    return {**json.loads(reply['result']), '_target': target}


def click(store, key):
    page = observe(store)
    matches = [x for x in page['controls'] if x['key'] == key]
    if len(matches) != 1:
        raise ValueError('可见可用控件不唯一: '+key)
    selector = matches[0]['selector']
    try:
        cli('page', 'click', '--store-id', store, '--target-id', page['_target'],
            '--selector', selector, structured=False)
    except Exception:
        # Some Shopee controls are visible and enabled but the CDP click action
        # rejects them as non-interactable.  Keep the same observed selector and
        # require exactly one visible element before using the page's own click.
        script = """(() => JSON.stringify((() => {
          const selector = %s;
          const nodes = [...document.querySelectorAll(selector)].filter((el) => {
            const style = getComputedStyle(el), rect = el.getBoundingClientRect();
            return style.display !== 'none' && style.visibility !== 'hidden' &&
              rect.width > 0 && rect.height > 0 && !el.disabled;
          });
          if (nodes.length !== 1) return {ok:false, count:nodes.length};
          nodes[0].click();
          return {ok:true, count:1};
        })()))()""" % json.dumps(selector)
        reply = cli('page', 'exec', '--store-id', store, '--target-id', page['_target'],
                    '--script', script)
        result = reply.get('result', {})
        if isinstance(result, str):
            result = json.loads(result)
        if reply.get('exceptionDetails') or result.get('ok') is not True:
            raise ValueError('页面内点击未确认: '+key)


def dates(page):
    found = re.findall(r'\d{2}/\d{2}/\d{4}', page['dateText'])
    if len(found) not in (1, 2) or 'GMT-03' not in page['dateText']:
        raise ValueError('经营日期或 GMT-03 不明确')
    return day(found[0]), day(found[-1])


def dates_are(page, start, end):
    try:
        return dates(page) == (start, end)
    except ValueError:
        return False


def ads_dates(page, target):
    try:
        a, b = (datetime.fromtimestamp(int(page[k]), BR) for k in ('from', 'to'))
    except (TypeError, ValueError, OverflowError):
        return False
    return (a.isoformat() == target+'T00:00:00-03:00' and b.isoformat() == target+'T23:59:59-03:00'
            and 'GMT−3' in page['adsDate'])


def ads_target_url(target):
    start = datetime.combine(date.fromisoformat(target), datetime.min.time(), BR)
    return f'{ADS}?from={int(start.timestamp())}&to={int(start.timestamp())+86399}&type=new_cpc_homepage&group=yesterday'


def metrics(text, ads=False):
    labels = ({'adCost': '(?:花费|Custo)', 'roi': '(?:广告支出回报率|ROAS)', 'attributedSales': '(?:销售额|Vendas)'}
              if ads else {'sales': '(?:销售额|Vendas)', 'orders': '(?:订单数|订单量|Pedidos)'})
    result = {}
    for key, label in labels.items():
        match = re.search(r'(?:^|\n)'+label+r'\n(?:(?:Definition updated|Definição atualizada) \d+\n)?([^\n]+)', text)
        result[key] = number(match[1]) if match else None
    if not ads and result['orders'] is not None:
        if result['orders'] != result['orders'].to_integral_value():
            raise ValueError('页面订单数不是整数')
        result['orders'] = int(result['orders'])
    return result


def ready(args, url, expected=None):
    started = time.monotonic()
    deadline, previous, hits = started+args.ready_wait, None, 0
    reason = '尚未读取页面'
    while time.monotonic() < deadline:
        page = observe(args.store_id)
        keys = {x['key'] for x in page['controls']}
        if {'login', 'verify'}.intersection(keys):
            raise ValueError('登录/验证入口可见，交回 Agent 处理')
        if page['proxyError']:
            raise ValueError('代理错误，交回 Agent 处理')
        valid = page['url'] == url and args.expected_account in [x.strip() for x in page['identityLines']]
        valid = valid and not page['loading'] and not {'verify', 'login'}.intersection(keys)
        if url == BUSINESS:
            valid = valid and ('订单类型\n已下订单' in page['business'] or 'Tipo de Pedido\nPedido Feito' in page['business'])
        reason = ('身份匹配='+str(args.expected_account in [x.strip() for x in page['identityLines']])+
                  ', 目标页面='+str(page['url'] == url)+', Local Seller='+str(page['localSeller'])+
                  ', 加载中='+str(page['loading']))
        try:
            if expected is not None:
                valid = valid and (dates(page) == expected if url == BUSINESS else ads_dates(page, args.date))
            valid = valid and bool(page['business'] if url == BUSINESS else page['ads'])
            if url == ADS:
                valid = valid and all(v is not None for v in metrics(page['ads'], True).values())
        except ValueError:
            valid = False
        signature = (page['business'], page['ads'], page['dateText'], page['adsDate'])
        hits = hits+1 if valid and signature == previous else 0
        if hits >= 2 and (expected is None or time.monotonic()-started >= 15):
            return page
        previous = signature if valid else None
        time.sleep(5)
    raise ValueError('身份、日期或页面加载未就绪；'+reason)


def evidence(page):
    result = {k: page[k] for k in ('url', 'identityLines', 'business', 'ads', 'dateText', 'adsDate', 'updating', 'localSeller')}
    if page['url'] == ADS and page.get('from') and page.get('to'):
        result['verifiedPeriod'] = [datetime.fromtimestamp(int(page[k]), BR).isoformat() for k in ('from', 'to')]
    return {**result, 'observedAt': now()}


def files(folder, suffix):
    return {str(p.resolve()): [p.stat().st_size, p.stat().st_mtime_ns]
            for p in folder.iterdir() if p.is_file() and p.suffix.lower() == suffix}


def wait_file(folder, baseline, suffix, seconds):
    until, previous = time.monotonic()+seconds, None
    while time.monotonic() < until:
        changed = {p: value for p, value in files(folder, suffix).items() if baseline.get(p) != value}
        if len(changed) > 1:
            raise ValueError('出现多份新下载，不能猜选最新文件')
        if changed and changed == previous:
            return Path(next(iter(changed)))
        previous = changed
        time.sleep(3)
    raise TimeoutError('已提交导出但文件尚未落盘；续跑不重复提交')


def new_task(tasks, old, target):
    stamp_date = date.fromisoformat(target).strftime('%d/%m/%Y')
    matches = [x for x in tasks if x['stamp'] not in old
               and 'Shopee-广告-所有-广告-组-数据-'+stamp_date+'-'+stamp_date+'.csv' in x['text']]
    if len(matches) > 1:
        raise ValueError('本次新增广告任务不唯一')
    return matches[0] if matches else None


def acquire(args, item, kind, page, download, capture, checkpoint, state, start, end):
    known = [read_json(p) for p in read_json(capture).get('downloads', []) if read_json(p)['type'] == kind]
    if len(known) > 1:
        raise ValueError('同类导出归档不唯一')
    if known:
        item['archive'] = known[0]
    suffix = '.xlsx' if kind == '店铺经营' else '.csv'
    if not item.get('archive'):
        if not item.get('submittedAt'):
            if kind == '广告组数据':
                click(args.store_id, 'adsExport')
                click(args.store_id, 'adsGroup')
                page = observe(args.store_id)
            item.update(submittedAt=now(), baseline=files(download, suffix), page=evidence(page),
                        oldTasks=[x['stamp'] for x in page['tasks']])
            write_json(checkpoint, state)  # Persist intent before external submission.
            click(args.store_id, 'businessExport' if kind == '店铺经营' else 'adsConfirm')
            log(kind+'导出已提交')
        if kind == '广告组数据' and not item.get('downloadClickedAt'):
            until = time.monotonic()+args.max_wait
            while time.monotonic() < until:
                current = observe(args.store_id)
                task = new_task(current['tasks'], item['oldTasks'], args.date)
                if task and task['downloadable']:
                    item.update(taskStamp=task['stamp'], downloadClickedAt=now())
                    write_json(checkpoint, state)
                    cli('page', 'click', '--store-id', args.store_id, '--target-id', current['_target'],
                        '--selector', task['selector'], structured=False)
                    break
                if not any(x['visible'] for x in current['tasks']):
                    click(args.store_id, 'tasks')
                time.sleep(5)
            else:
                raise TimeoutError('广告任务仍生成中；保存断点，不重复提交')
        source = wait_file(download, item['baseline'], suffix, args.max_wait)
        if kind == '店铺经营':
            parse_business(source, args.expected_account, start, end, args.date)
        else:
            parse_ads(source, args.expected_account, args.date)
        item['archive'] = archive(capture, args.store_id, source, download, kind, start, end)
        write_json(checkpoint, state)
    entry = item['archive']
    if digest(Path(entry['path'])) != entry['sha256']:
        raise ValueError('归档文件哈希不一致')
    return (parse_business(entry['path'], args.expected_account, start, end, args.date) if kind == '店铺经营'
            else parse_ads(entry['path'], args.expected_account, args.date))


def run(args):
    root = Path(args.output_root).resolve()
    folder = root/'数据'/args.date/'Shopee采集'/args.store_id
    folder.mkdir(parents=True, exist_ok=True)
    checkpoint, capture, lock = folder/'checkpoint.json', folder/'capture.json', folder/'running.lock'
    with lock.open('x', encoding='utf-8') as handle:
        handle.write(str(os.getpid()))
    try:
        return run_locked(args, root, folder, checkpoint, capture)
    finally:
        lock.unlink()


def run_locked(args, root, folder, checkpoint, capture):
    identity = {'storeId': args.store_id, 'name': args.expected_name, 'account': args.expected_account,
                'dateIso': args.date, 'collectAds': args.collect_ads, 'orderMetric': 'orders'}
    state = {}
    result = {**identity, 'sales': None, 'orders': None, 'adCost': None, 'roi': None, 'missingReason': {}}
    state = read_json(checkpoint) if checkpoint.exists() else {'identity': identity, 'items': {}}
    if state['identity'] != identity:
        raise ValueError('断点店铺、日期或广告配置不一致；保留原任务不改写')
    try:
        if state.get('status') in ('success', 'collected'):
            for item in state['items'].values():
                if digest(Path(item['archive']['path'])) != item['archive']['sha256']:
                    raise ValueError('已完成归档文件发生变化')
            log('复用已完成结果，不开店、不导出')
            return read_json(capture)
        if not capture.exists():
            write_json(capture, {'dateIso': args.date, 'configSnapshot': {'report': {'outputRoot': str(root)},
                        'stores': [{'storeId': args.store_id}]}, 'downloads': []})
        download = Path(args.download_dir).resolve(strict=True)
        if state.get('downloadDir', str(download)) != str(download):
            raise ValueError('本店下载目录改变')
        state['downloadDir'] = str(download)
        try:
            cached = state.get('businessResult')
            refresh = bool(cached and (cached.get('missingReason') or cached['singlePage']['updating']))
            start = (date.fromisoformat(args.date)-timedelta(days=6)).isoformat()
            if cached:
                entry = state['items']['business']['archive']
                if not Path(entry['path']).is_file() or digest(Path(entry['path'])) != entry['sha256']:
                    raise ValueError('经营归档文件缺失或哈希不一致')
                if refresh:
                    parsed = parse_business(entry['path'], args.expected_account, start, args.date, args.date)
                    current = ready(args, BUSINESS)
            elif state['items'].get('business', {}).get('archive'):
                entry = state['items']['business']['archive']
                parsed = parse_business(entry['path'], args.expected_account, start, args.date, args.date)
                current = ready(args, BUSINESS)
            else:
                current = ready(args, BUSINESS)
                if dates_are(current, start, args.date):
                    page = ready(args, BUSINESS, (start, args.date))
                else:
                    click(args.store_id, 'businessDate')
                    click(args.store_id, '过去7天')
                    page = ready(args, BUSINESS, (start, args.date))
                parsed = acquire(args, state['items'].setdefault('business', {}), '店铺经营', page, download,
                                 capture, checkpoint, state, start, args.date)
            if not cached or refresh:
                current = ready(args, BUSINESS)
                if dates_are(current, args.date, args.date):
                    single = ready(args, BUSINESS, (args.date, args.date))
                else:
                    click(args.store_id, 'businessDate')
                    click(args.store_id, '昨天')
                    single = ready(args, BUSINESS, (args.date, args.date))
                checked = {**metrics(single['business']), 'updating': single['updating']}
                state['businessResult'] = {**serializable(reconcile(parsed, checked)),
                                           'businessExport': serializable(parsed), 'singlePage': evidence(single)}
                write_json(checkpoint, state)
            result.update(state['businessResult'])
        except Exception as error:
            return handoff(folder, identity, '经营采集：'+str(error))
        if args.collect_ads:
            try:
                current = observe(args.store_id)
                if current['url'] != ADS:
                    visit(args.store_id, ads_target_url(args.date))
                    current = ready(args, ADS, (args.date, args.date))
                if ads_dates(current, args.date):
                    page = ready(args, ADS, (args.date, args.date))
                else:
                    click(args.store_id, 'adsDate')
                    click(args.store_id, '昨天')
                    page = ready(args, ADS, (args.date, args.date))
                parsed = acquire(args, state['items'].setdefault('ads', {}), '广告组数据', page, download,
                                 capture, checkpoint, state, args.date, args.date)
                result['adsExport'] = parsed
                result['adsPage'] = evidence(page)
                result.update(serializable(metrics(page['ads'], True)))
                result['adSource'] = 'single-day-page-fallback'
                result['adSourceReason'] = parsed['aggregationReason']
            except Exception as error:
                return handoff(folder, identity, '广告采集：'+str(error))
        result['status'] = 'needs_agent' if result['missingReason'] or args.collect_ads or result['singlePage']['updating'] else 'collected'
    except Exception as error:
        return handoff(folder, identity, error)
    result.update(capturedAt=now(), timeZone='America/Sao_Paulo', currency='BRL')
    previous = read_json(capture) if capture.exists() else {}
    if previous.get('capturedAt'):
        stamp = datetime.fromisoformat(previous['capturedAt']).strftime('%Y%m%d_%H%M%S')
        history = folder/'attempts'/(stamp+'.json')
        if not history.exists():
            write_json(history, previous)
    write_json(capture, {**{k: previous[k] for k in ('configSnapshot', 'downloads') if k in previous}, **result})
    state['status'] = result['status']
    write_json(checkpoint, state)
    write_json(folder/'handoff.json', {**identity, 'status': result['status'], 'reason': 'Agent 核验候选值和页面来源，决定采用及关店'})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('store-id', 'expected-name', 'expected-account', 'output-root'):
        parser.add_argument('--'+key, required=True)
    parser.add_argument('--date', default=(datetime.now(BR).date()-timedelta(days=1)).isoformat())
    parser.add_argument('--collect-ads', action='store_true')
    parser.add_argument('--ready-wait', type=int, default=90)
    parser.add_argument('--max-wait', type=int, default=60)
    parser.add_argument('--download-dir', required=True, help='Agent 从本店开店返回值获取的下载目录')
    args = parser.parse_args()
    date.fromisoformat(args.date)
    if not args.store_id.isdigit() or not args.expected_account.strip() or min(args.ready_wait, args.max_wait) < 10:
        parser.error('账号、店铺 ID 或等待参数无效')
    result = run(args)
    log(json.dumps({k: result.get(k) for k in ('status', 'storeId', 'reason', 'missingReason')}, ensure_ascii=False))
    return 0 if result['status'] in ('success', 'collected') else 2


if __name__ == '__main__':
    raise SystemExit(main())
