"""Builds a launcher-style 26.3 game dir (client jar, Linux libraries, assets) from Mojang's piston-meta, sha1-checked."""
import hashlib, json, sys, urllib.request, concurrent.futures as cf
from pathlib import Path

root = Path(sys.argv[1])
def get(url):
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read()
def fetch(url, sha1, dest):
    dest = Path(dest)
    if dest.exists() and hashlib.sha1(dest.read_bytes()).hexdigest() == sha1:
        return
    data = get(url)
    assert hashlib.sha1(data).hexdigest() == sha1, f'sha1 mismatch {url}'
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)

manifest = json.loads(get('https://piston-meta.mojang.com/mc/game/version_manifest_v2.json'))
entry = next(v for v in manifest['versions'] if v['id'] == '26.3')
raw = get(entry['url'])
assert hashlib.sha1(raw).hexdigest() == entry['sha1']
version = json.loads(raw)
vdir = root / 'versions/26.3'
vdir.mkdir(parents=True, exist_ok=True)
(vdir / '26.3.json').write_bytes(raw)
jobs = [(version['downloads']['client']['url'], version['downloads']['client']['sha1'], vdir / '26.3.jar'),
        (version['downloads']['server']['url'], version['downloads']['server']['sha1'], root / 'server.jar')]
for lib in version['libraries']:
    rules = lib.get('rules')
    if rules and not any(r['action'] == 'allow' and r.get('os', {}).get('name', 'linux') == 'linux' for r in rules):
        continue
    art = lib.get('downloads', {}).get('artifact')
    if art:
        jobs.append((art['url'], art['sha1'], root / 'libraries' / art['path']))
index_info = version['assetIndex']
index_raw = get(index_info['url'])
assert hashlib.sha1(index_raw).hexdigest() == index_info['sha1']
(root / 'assets/indexes').mkdir(parents=True, exist_ok=True)
(root / f"assets/indexes/{index_info['id']}.json").write_bytes(index_raw)
for obj in json.loads(index_raw)['objects'].values():
    h = obj['hash']
    jobs.append((f'https://resources.download.minecraft.net/{h[:2]}/{h}', h, root / f'assets/objects/{h[:2]}/{h}'))
with cf.ThreadPoolExecutor(16) as pool:
    list(pool.map(lambda j: fetch(*j), jobs))
print(f'{len(jobs)} files verified in {root}')
