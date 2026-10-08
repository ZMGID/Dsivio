"""Complete the report table with compact pictures and native links."""
import io
import json
import posixpath
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from PIL import Image
from embed_images import S, R, P, X, child, embed, serialize, sheets, xml
from prepare_images import prepare


def finalize(input_path, base, output, columns):
    data = json.loads(input_path.read_text(encoding='utf-8'))
    candidates = data['candidates']
    work = output.parent
    manifest = []
    for i, c in enumerate(candidates):
        source = Path(c['image'])
        if not source.is_absolute():
            source = input_path.parent / source
        target = work / (c['id'] + '.jpg')
        prepare(source, target)
        manifest.append(dict(sheet='竞品调研表', cell=f'{columns["image"]}{i+7}', productId=c['id'], path=str(target)))
    with ZipFile(base) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    sheet_path = sheets(parts)['竞品调研表']
    root = xml(parts[sheet_path])
    rel_path = posixpath.join(posixpath.dirname(sheet_path), '_rels', posixpath.basename(sheet_path) + '.rels')
    rels = xml(parts[rel_path]) if rel_path in parts else xml(f'<Relationships xmlns="{P}"/>'.encode())
    links = child(root, S, 'hyperlinks')
    # OOXML requires hyperlinks before print settings and page margins.
    root.remove(links)
    later = {'printOptions', 'pageMargins', 'pageSetup', 'headerFooter', 'rowBreaks', 'colBreaks',
             'customProperties', 'cellWatches', 'ignoredErrors', 'smartTags', 'drawing', 'extLst'}
    at = next((i for i, node in enumerate(root) if node.tag.split('}')[-1] in later), len(root))
    root.insert(at, links)
    for i, c in enumerate(candidates):
        rid = f'researchLink{i}'
        link = child(links, S, 'hyperlink', ref=f'{columns["url"]}{i+7}')
        link.set(f'{{{R}}}id', rid)
        child(rels, P, 'Relationship', Id=rid, Type=R+'/hyperlink', Target=c['url'], TargetMode='External')
    parts[sheet_path], parts[rel_path] = serialize(root), serialize(rels)
    linked = work / 'linked.xlsx'
    with ZipFile(linked, 'w', ZIP_DEFLATED) as archive:
        for name, content in parts.items():
            archive.writestr(name, content)
    embed(linked, manifest, output)
    return verify(output, candidates, columns)


def verify(output, candidates, columns):
    with ZipFile(output) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    paths = sheets(parts)
    if len(paths) != 1:
        raise ValueError('Expected a single worksheet')
    root = xml(parts[paths['竞品调研表']])
    if max(int(row.get('r')) for row in root.findall(f'.//{{{S}}}row')) != len(candidates)+6:
        raise ValueError('Unexpected report rows')
    strings = [''.join(node.itertext()) for node in xml(parts['xl/sharedStrings.xml'])] if 'xl/sharedStrings.xml' in parts else []

    def value(ref):
        cell = root.find(f'.//{{{S}}}c[@r="{ref}"]')
        v = cell.find(f'{{{S}}}v')
        if cell.get('t') == 'inlineStr':
            return ''.join(cell.find(f'{{{S}}}is').itertext())
        return strings[int(v.text)] if cell.get('t') == 's' else v.text if v is not None else ''

    for i, c in enumerate(candidates):
        row = i+7
        if float(value(f'{columns["price"]}{row}')) != c['price'] or value(f'{columns["url"]}{row}') != c['url']:
            raise ValueError(f'Price/link mismatch: {c["id"]}')
        if not value(f'{columns["package"]}{row}').startswith(f'{c["qty"]}{c["unit"]}／{c["salesUnit"]}'):
            raise ValueError(f'Package mismatch: {c["id"]}')
    for name, content in parts.items():
        if name.endswith('.rels'):
            parent = '' if name == '_rels/.rels' else posixpath.dirname(posixpath.dirname(name))
            for rel in xml(content):
                if rel.get('TargetMode') != 'External':
                    target = posixpath.normpath(posixpath.join(parent, rel.get('Target'))).lstrip('/')
                    if target not in parts:
                        raise ValueError(f'Broken relationship: {target}')
    anchors = [a for name in parts if name.startswith('xl/drawings/') and '/_rels/' not in name and name.endswith('.xml')
               for a in xml(parts[name]).findall(f'{{{X}}}twoCellAnchor')]
    if len(anchors) != len(candidates) or len(root.findall(f'.//{{{S}}}hyperlink')) != len(candidates):
        raise ValueError('Picture/link count mismatch')
    for i, anchor in enumerate(anchors):
        marker = anchor.find(f'{{{X}}}from')
        if marker.find(f'{{{X}}}row').text != str(i+6) or marker.find(f'{{{X}}}col').text != str(ord(columns['image'])-65):
            raise ValueError('Picture/candidate alignment mismatch')
    media = [content for name, content in parts.items() if name.startswith('xl/media/')]
    for content in media:
        with Image.open(io.BytesIO(content)) as image:
            if image.size != (320,320) or image.format != 'JPEG' or len(content) > 40960:
                raise ValueError('Invalid compressed picture')
    return dict(sheetCount=1,pictureCount=len(anchors),imageBytes=sum(map(len,media)),fileChecks=True)


if __name__ == '__main__':
    print(json.dumps(finalize(*(Path(arg).resolve() for arg in sys.argv[1:4]), json.loads(sys.argv[4]))))
