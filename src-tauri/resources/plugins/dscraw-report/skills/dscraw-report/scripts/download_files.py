"""Archive one confirmed download; expire recorded originals after three calendar days."""
import argparse
from datetime import date, datetime, timedelta
import hashlib
import json
from pathlib import Path
import shutil

from report_state import read_json, write_json, require, safe_name


def digest(path):
    with path.open('rb') as handle:
        result = hashlib.sha256()
        for chunk in iter(lambda: handle.read(1024 * 1024), b''):
            result.update(chunk)
        return result.hexdigest()


def inside(path, root):
    return path.resolve().is_relative_to(root.resolve())


def archive(report_file, store_id, source, download_dir, data_type, start, end):
    report = read_json(report_file)
    require(store_id in [s['storeId'] for s in report['configSnapshot']['stores']], '店铺不在本次配置中')
    require(safe_name(data_type), '数据类型名称无效')
    require(date.fromisoformat(start) <= date.fromisoformat(end), '导出日期区间无效')
    source, download_dir = Path(source).resolve(), Path(download_dir).resolve()
    root = Path(report['configSnapshot']['report']['outputRoot']).resolve()
    require(source.is_file() and inside(source, download_dir), '来源不在本店下载目录')
    require(source.suffix.lower() in ('.xlsx', '.xls', '.csv', '.zip'), '只归档已完成的导出文件')
    now = datetime.now().astimezone()
    stamp = now.strftime('%Y%m%d_%H%M%S_%f')
    folder = root / '数据' / date.fromisoformat(report['dateIso']).isoformat()
    destination = folder / '原始导出' / store_id / data_type / f'{start}_{end}' / stamp / source.name
    require(inside(destination, root), '归档路径超出指定目录')
    require(not inside(source, root), '文件已在正式目录，请复用原下载清单')
    original_hash = digest(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    require(digest(destination) == original_hash, '归档副本不完整，保留下载原件')
    manifest = folder / f'下载清单_{stamp}.json'
    entry = {'storeId': store_id, 'type': data_type, 'start': start, 'end': end,
             'source': str(source), 'downloadDir': str(download_dir), 'path': str(destination), 'sha256': original_hash,
             'collectedAt': now.isoformat(timespec='seconds'), 'status': '中转待清理', 'reason': ''}
    write_json(manifest, entry)
    report.setdefault('downloads', []).append(str(manifest))
    write_json(report_file, report)
    try:
        require(digest(source) == original_hash, '下载原件已变化，保留原件')
        source.unlink()
        entry['status'] = '已归档'
    except (OSError, ValueError) as error:
        entry['status'], entry['reason'] = '中转待清理', str(error)
    write_json(manifest, entry)
    return entry


def check_downloads(report):
    require(isinstance(report.get('downloads'), list), '结果中须列出 downloads；仅页面取数填空数组')
    root = Path(report['configSnapshot']['report']['outputRoot']).resolve()
    for filename in report['downloads']:
        manifest = Path(filename)
        require(inside(manifest, root / '数据'), '下载清单不在指定目录')
        entry = read_json(manifest)
        path = Path(entry['path'])
        require(inside(path, root / '数据') and path.is_file(), '归档文件缺失')
        require(entry['status'] in ('已归档', '中转待清理'), '下载尚未归档')


def cleanup(output_root, now=None):
    root = Path(output_root).resolve()
    now = now or datetime.now().astimezone()
    cutoff = now.date() - timedelta(days=2)
    result = {'deleted': 0, 'pending': []}
    for manifest in (root / '数据').glob('*/下载清单_*.json'):
        try:
            require(inside(manifest, root / '数据'), '清单路径超出指定目录')
            entry = read_json(manifest)
            if entry.get('status') == '已过期清理':
                continue
            if entry.get('status') == '中转待清理':
                source = Path(entry['source'])
                require(inside(source, Path(entry['downloadDir'])), '中转路径超出本店下载目录')
                require(inside(Path(entry['path']), manifest.parent / '原始导出') and
                        digest(Path(entry['path'])) == entry['sha256'], '归档副本不可用，保留中转原件')
                if source.exists():
                    require(source.is_file() and digest(source) == entry['sha256'], '中转原件已变化，跳过')
                    source.unlink()
                entry.update(status='已归档', reason='')
                write_json(manifest, entry)
            captured = datetime.fromisoformat(entry['collectedAt'])
            require(captured.tzinfo is not None, '采集时间缺少时区')
            if captured.astimezone(now.tzinfo).date() >= cutoff:
                continue
            path = Path(entry['path'])
            archive_root = manifest.parent / '原始导出'
            require(inside(path, archive_root) and inside(archive_root, root), '清理路径超出原始导出目录')
            if path.exists():
                require(path.is_file() and digest(path) == entry['sha256'], '原始导出内容已变化，跳过')
                path.unlink()
                result['deleted'] += 1
            entry.update(status='已过期清理', deletedAt=now.isoformat(timespec='seconds'))
            write_json(manifest, entry)
            parent = path.parent
            while parent != archive_root and inside(parent, archive_root):
                try:
                    parent.rmdir()  # Only remove empty directories.
                except OSError:
                    break
                parent = parent.parent
        except (OSError, ValueError, KeyError, TypeError) as error:
            result['pending'].append(f'{manifest}: {error}')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_json')
    parser.add_argument('store_id')
    parser.add_argument('source')
    parser.add_argument('--download-dir', required=True)
    parser.add_argument('--type', required=True)
    parser.add_argument('--start', required=True)
    parser.add_argument('--end', required=True)
    args = parser.parse_args()
    print(json.dumps(archive(args.report_json, args.store_id, args.source, args.download_dir, args.type, args.start, args.end), ensure_ascii=False))
