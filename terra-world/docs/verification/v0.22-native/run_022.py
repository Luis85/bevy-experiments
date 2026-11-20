#!/usr/bin/env python3
"""Record owned native review processes without treating candidates as acceptance."""
import argparse,datetime,hashlib,json,os,subprocess,sys,time
from pathlib import Path
sys.dont_write_bytecode=True

ROOT=Path('/workspace/scratch/96f1d170dd13/source/terra-world')
OUT=Path('/workspace/scratch/96f1d170dd13/source/terra-world/docs/verification/v0.22-native')
sys.path.insert(0,str(ROOT/'scripts'))
from verify import source_inputs

def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def canonical_digest(value):
    encoded=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False).encode('utf-8')
    return hashlib.sha256(encoded).hexdigest()
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def main():
    p=argparse.ArgumentParser()
    p.add_argument('--phase',choices=['baseline','candidate','confirmation','package_confirmation'],default='candidate')
    p.add_argument('--binary',default='/tmp/terra-target/debug/terra-demo')
    p.add_argument('--cwd',default=str(ROOT))
    p.add_argument('--trace-log')
    p.add_argument('--expected-interruption',action='store_true')
    p.add_argument('name');p.add_argument('args',nargs=argparse.REMAINDER)
    a=p.parse_args();args=a.args[1:] if a.args[:1]==['--'] else a.args
    if a.phase=='confirmation' and a.trace_log:
        p.error('Native confirmation requires a direct process PID; use an untraced launch.')
    prefix=OUT/a.name
    for suffix in ('-process.json','-start.json','.log'):
        existing=Path(str(prefix)+suffix)
        if existing.exists() or existing.is_symlink():raise FileExistsError(existing)
    log=Path(str(prefix)+'.log')
    frozen=source_inputs()
    command=[a.binary,*args]
    if a.trace_log:command=['strace','-f','-qq','-e','trace=file','-o',a.trace_log,*command]
    env=os.environ.copy();env.update(DISPLAY=':103',WGPU_BACKEND='vulkan',XDG_RUNTIME_DIR='/tmp/terra-native022/xdg')
    Path(env['XDG_RUNTIME_DIR']).mkdir(parents=True,exist_ok=True,mode=0o700)
    record={'release':'0.21.0' if a.phase=='baseline' else '0.22.0','phase':a.phase,'command':command,'working_directory':a.cwd,'started_utc':now(),'binary_sha256':digest(a.binary),'source_input_count':len(frozen),'expected_interruption':a.expected_interruption,'trace_file':a.trace_log}
    if a.phase!='candidate':
        record.update(schema=1,source_inputs_sha256=canonical_digest(frozen))
    started=time.monotonic()
    with log.open('xb') as output:
        process=subprocess.Popen(command,cwd=a.cwd,env=env,stdout=output,stderr=subprocess.STDOUT)
        record['pid']=process.pid
        with Path(str(prefix)+'-start.json').open('x') as receipt:
            json.dump(record,receipt,indent=2);receipt.write('\n')
        print(json.dumps({'name':a.name,'pid':process.pid}),flush=True)
        code=process.wait()
    finished_sources=source_inputs()
    record.update(exit_code=code,finished_utc=now(),seconds=time.monotonic()-started,source_inputs_unchanged=finished_sources==frozen,log_sha256=digest(log))
    if a.phase!='candidate':record['finished_source_inputs_sha256']=canonical_digest(finished_sources)
    if a.trace_log:record['trace_sha256']=digest(a.trace_log)
    with Path(str(prefix)+'-process.json').open('x') as receipt:
        json.dump(record,receipt,indent=2);receipt.write('\n')
    print(json.dumps(record,indent=2),flush=True)
    if code!=0 and not(a.expected_interruption and code<0):sys.exit(1)
if __name__=='__main__':main()
