import hashlib,json,os,re,shutil,socket,subprocess,sys,time
from pathlib import Path
root=Path('/home/doc/code/Ferraloom');os.chdir(root)
host=socket.gethostname();assert host=='ferraloom-builder',host
helper_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
assert len(sys.argv)==3
source=sys.argv[1];assert re.fullmatch('[a-f0-9]{40}',source)
prefix=sys.argv[2];assert re.fullmatch('review29-all16-[a-z0-9-]+',prefix)
receipt=root/('.cache/'+prefix+'-native.json');assert not receipt.exists()
def source_pin(full=False):
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==source
 assert not subprocess.check_output(['git','status','--porcelain']+([] if full else ['--untracked-files=no']),text=True)
source_pin(True)
group=Path('/proc/self/cgroup').read_text().strip().split('::',1)[1];control=Path('/sys/fs/cgroup')/group.lstrip('/')
limits={n:(control/n).read_text().strip() for n in ('memory.max','memory.swap.max','cpu.max')};assert limits=={'memory.max':str(6*1024**3),'memory.swap.max':'0','cpu.max':'200000 100000'}
env=os.environ.copy();assert env.get('CARGO_BUILD_JOBS')=='1',env.get('CARGO_BUILD_JOBS');env['PUMPKIN_MINECRAFT_SERVER_JAR']=str(root/'.cache/vanilla/26.3/server.jar');env['CARGO_TERM_COLOR']='never';env['CARGO_INCREMENTAL']='0'
common=['--manifest-path','vendor/pumpkin/Cargo.toml','--locked','--offline','--jobs','1']
steps=[
 ('fmt-root',['cargo','fmt','--all','--','--check']),
 ('fmt-vendor',['cargo','fmt','--manifest-path','vendor/pumpkin/Cargo.toml','--all','--','--check']),
 ('python',['python3','-B','-m','unittest','discover','-s','tests']),
 ('vendor-clippy',['cargo','clippy',*common,'--keep-going','--workspace','--all-targets','--target-dir','target','--','-D','warnings']),
 ('data',['cargo','test',*common,'-p','pumpkin-data','--lib','--target-dir','target']),
 ('protocol',['cargo','test',*common,'-p','pumpkin-protocol','--lib','--tests','--target-dir','target']),
 ('command',['cargo','test','--locked','--offline','--jobs','1','-p','pumpkin-command','--lib']),
 ('world',['cargo','test',*common,'-p','pumpkin-world','--lib','--target-dir','target','--','--test-threads=2']),
 ('inventory',['cargo','test',*common,'-p','pumpkin-inventory','--lib','--target-dir','target']),
 ('core',['cargo','test',*common,'-p','pumpkin-core','--lib','--target-dir','target','--','--test-threads=2']),
 ('gametest-compile',['cargo','check',*common,'-p','pumpkin-gametest','--all-targets','--target-dir','target']),
 ('clippy',['cargo','clippy','--locked','--offline','--jobs','1','--all-targets','--','-D','warnings']),
 ('wrapper',['cargo','test','--locked','--offline','--jobs','1']),
 ('node-protocol',['npm','run','test:protocol']),
 ('typescript',['npm','run','check:protocol']),
 ('build',['cargo','build','--locked','--offline','--jobs','1']),
]
# Planned counts: B1612 + terminal11 + world7 + crafting7 = Rust1637.
# Python111 + Dragon9 + inventory6 + crafting2 =128; Node66 + Dragon4 =70.
# Source proposal only: run after ROOT binds a reviewed future clean40SHA.
# Update explicitly if reviewed test additions change them; never accept zero matches.
expected={'data':[96],'protocol':[129,12],'command':[92],'world':[416],'inventory':[24],'core':[856],'wrapper':[12]}
summary_re=re.compile(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;')
header_re=re.compile(r'^\s*Running .+ \(([^()]+)\)\s*$',re.M)
execution_checks={};records=[];total=time.monotonic()
result={'host':host,'helper_sha256':helper_sha256,'source_commit':source,'resource_limits':limits,'cgroup':group,'build_environment':{'CARGO_INCREMENTAL':env['CARGO_INCREMENTAL'],'CARGO_BUILD_JOBS':env['CARGO_BUILD_JOBS']},'steps':records,'execution_checks':execution_checks,'scope':'All seven vendored crates plus wrapper, Python, Node and TypeScript. GameTest compile only, zero standalone behavior tests. No parity promotion.'}
for name,argv in steps:
 source_pin();assert shutil.disk_usage(root).free>=2*1024**3
 log=root/('.cache/'+prefix+'-'+name+'.log');start=time.monotonic();print(json.dumps({'step':name,'source':source,'command':argv}),flush=True)
 with log.open('x') as stream:
  try:code=subprocess.run(argv,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=1800).returncode
  except subprocess.TimeoutExpired:code=124
 text=log.read_text();checks=[];gate=True
 if name in expected:
  headers=list(header_re.finditer(text));gate=len(headers)==len(expected[name]);summaries=summary_re.findall(text);gate=gate and not any(s[0]!='ok' or int(s[2]) for s in summaries)
  if gate:
   for i,(header,count) in enumerate(zip(headers,expected[name],strict=True)):
    segment=text[header.end():headers[i+1].start() if i+1<len(headers) else len(text)];found=summary_re.findall(segment)
    final=found[-1] if found else None
    valid=final==('ok',str(count),'0','0','0','0');gate=gate and valid
    binary=(root/header.group(1)).resolve();assert binary.is_relative_to(root/'target/debug/deps')
    checks.append({'binary':str(binary),'expected_passed':count})
  if gate:execution_checks[name]=checks
 elif name=='gametest-compile':gate=not summary_re.search(text)
 elif name=='python':gate=bool(re.search(r'Ran 128 tests in ',text)) and bool(re.search(r'^OK$',text,re.M))
 elif name=='node-protocol':gate=any(all(re.search(r'^'+re.escape(marker)+' '+field+' '+str(value)+r'$',text,re.M) for field,value in {'tests':70,'suites':0,'pass':70,'fail':0,'cancelled':0,'skipped':0,'todo':0}.items()) for marker in ['#','ℹ'])
 if code==0 and not gate:code=125
 record={'step':name,'command':argv,'exit_code':code,'elapsed_seconds':round(time.monotonic()-start,3),'log':str(log),'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'execution_gate_passed':gate};records.append(record);result['elapsed_seconds']=round(time.monotonic()-total,3)
 receipt.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(record),flush=True)
 if code:
  print(text[-20000:],flush=True);sys.exit(code)
source_pin()
generated=root/'vendor/pumpkin/crates/pumpkin-core/data';preserved=None
if generated.exists():
 files=sorted(p for p in generated.rglob('*') if p.is_file());assert [str(p.relative_to(generated)) for p in files]==['usercache.json'],files
 value=json.loads(files[0].read_text());assert len(value)==1 and value[0]['name']=='QueuedPlayer' and value[0]['uuid']=='00000000-0000-0000-0000-000000000066',value
 destination=root/('.cache/'+prefix+'-generated-data');assert not destination.exists();digest=hashlib.sha256(files[0].read_bytes()).hexdigest();generated.rename(destination);assert hashlib.sha256((destination/'usercache.json').read_bytes()).hexdigest()==digest;preserved={'path':str(destination/'usercache.json'),'sha256':digest}
source_pin(True);assert set(execution_checks)==set(expected)
result.update({'preserved_generated_test_artifact':preserved,'binary_sha256':hashlib.sha256((root/'target/debug/ferraloom').read_bytes()).hexdigest(),'full_source_clean':True,'elapsed_seconds':round(time.monotonic()-total,3)})
receipt.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'all16_passed':True,'source':source,'binary_sha256':result['binary_sha256'],'executed_Rust_tests':sum(sum(x['expected_passed'] for x in records) for records in execution_checks.values())}),flush=True)
