#!/usr/bin/env python3
"""Capture/check PowderSnowBlock against a disposable real vanilla ServerLevel."""
import argparse
import gzip
parser=argparse.ArgumentParser()
parser.add_argument("--check",action="store_true")
args=parser.parse_args()
from pathlib import Path
import os
import subprocess
import tempfile
import zipfile
import urllib.request
import time

HERE=Path(__file__).resolve().parent
VANILLA=Path(os.environ.get('FERRALOOM_VANILLA_DIR','/home/doc/code/Ferraloom/.cache/vanilla/26.3'))
JDK=Path(os.environ.get('JAVA_HOME','/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1'))/'bin'
MOCK_CACHE=VANILLA/'tree-harness'
BYTEBUDDY=MOCK_CACHE/'byte-buddy-1.15.4.jar'
if not BYTEBUDDY.exists():
    MOCK_CACHE.mkdir(parents=True,exist_ok=True)
    urllib.request.urlretrieve('https://repo.maven.apache.org/maven2/net/bytebuddy/byte-buddy/1.15.4/byte-buddy-1.15.4.jar',BYTEBUDDY)
classpath=':'.join(map(str,[VANILLA/'versions/26.3/server-26.3.jar',*sorted((VANILLA/'libraries').rglob('*.jar')),BYTEBUDDY]))
with tempfile.TemporaryDirectory(prefix='ferraloom-powder-snow-') as temp:
    path=Path(temp)
    subprocess.run([str(JDK/'javac'),'-cp',classpath,'-d',temp,str(HERE/'PowderSnowOracle.java')],check=True)
    agent=path/'oracle-agent.jar'
    with zipfile.ZipFile(agent,'w') as jar:
        jar.writestr('META-INF/MANIFEST.MF','Manifest-Version: 1.0\nPremain-Class: PowderSnowOracle\nCan-Retransform-Classes: true\n\n')
        for file in path.glob('*.class'):jar.write(file,file.name)
    (path/'eula.txt').write_text('eula=true\n')
    (path/'server.properties').write_text('online-mode=false\nlevel-seed=0\nserver-ip=127.0.0.1\nserver-port=0\nlevel-type=minecraft:flat\ngenerator-settings={"biome":"minecraft:plains","layers":[{"block":"minecraft:bedrock","height":1}]}\nspawn-protection=0\nview-distance=2\nsimulation-distance=2\nmax-tick-time=-1\npause-when-empty-seconds=-1\n')
    output=path/'powder-snow.tsv'
    with (path/'server.log').open('w') as log:
        process=subprocess.Popen([str(JDK/'java'),'-Xmx512M','-Dnet.bytebuddy.experimental=true','-javaagent:'+str(agent)+'='+str(output),'-cp',classpath,'net.minecraft.server.Main','--nogui'],cwd=path,stdout=log,stderr=subprocess.STDOUT)
        try:
            deadline=time.monotonic()+240
            while process.poll() is None and not (path/'oracle-done').exists() and not (path/'oracle-error.txt').exists():
                if time.monotonic()>=deadline:raise TimeoutError('powder-snow oracle did not complete in 240 seconds')
                time.sleep(0.1)
            if not (path/'oracle-done').exists():raise RuntimeError('powder-snow oracle exited before completing its fixtures')
        except Exception:
            print((path/'server.log').read_text()[-12000:]);raise
        finally:
            # The real callbacks have completed before the marker. Some queued
            # native chunk saves wait indefinitely during shutdown; this process
            # owns only the disposable fixture world, so bound its teardown.
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill();process.wait()
    if (path/'oracle-error.txt').exists():raise SystemExit((path/'oracle-error.txt').read_text())
    actual=output.read_bytes()
    golden=HERE/'powder-snow.tsv.gz'
    if args.check:
        if actual != gzip.decompress(golden.read_bytes()):
            raise SystemExit('real ServerLevel powder snow vectors differ')
    else:
        golden.write_bytes(gzip.compress(actual,mtime=0))
    print(f'{len(actual.splitlines())} real ServerLevel powder snow vectors PASS')
