#!/usr/bin/env python3
"""Publish derived native comparisons; private save/database contents stay private."""
import argparse,datetime,hashlib,json
from pathlib import Path
OUT=Path('/tmp/terra-native');ROOT=Path('/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next')
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def read(p):return json.loads(Path(p).read_text())
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def write(p,d):
 p=Path(p);assert p.parent==OUT and p.name.startswith('v16-confirm-') and not p.exists() and not p.is_symlink()
 with p.open('x') as f:json.dump(d,f,indent=2);f.write('\n')
 print(json.dumps({'output':str(p),'status':d['status'],'sha256':sha(p)},indent=2))
def confirmation(process,capture,phase='during_process'):
 start=read(OUT/(process+'-start.json'));cap=read(OUT/(capture+'-capture.json'))
 assert start['phase']=='confirmation' and start['schema']==1 and cap['window_pid_status']=='available' and cap['window_pid']==start['pid']
 gates=read(ROOT/'docs/verification/v0.16-final/gates.json');assert gates['status']=='passed' and start['binary_sha256']==gates['native_binary_sha256']
 if phase=='after_exit':
  done=read(OUT/(process+'-process.json'));assert done['exit_code']==0;observed=now();assert observed>=done['finished_utc']
 else:observed=cap['captured_utc']
 return {'process':process,'pid':start['pid'],'binary_sha256':start['binary_sha256'],'gate_sha256':sha(ROOT/'docs/verification/v0.16-final/gates.json'),'source_inputs_sha256':start['source_inputs_sha256'],'observed_utc':observed,'observation_phase':phase}
def main():
 p=argparse.ArgumentParser();p.add_argument('action',choices=['bind','guidance','originals']);p.add_argument('inputs',nargs='+');a=p.parse_args()
 if a.action=='bind':
  raw,output,process,capture=a.inputs;d=read(raw);assert d['status']=='passed' and d['release']=='0.16.0';d['confirmation']=confirmation(process,capture);assert d['completed_utc']>=d['confirmation']['observed_utc'];write(output,d)
 elif a.action=='guidance':
  before,after,output,process,capture=a.inputs;b=read(before);v=read(after)
  checks={'full_authoritative_state_equal':b['state']==v['state'],'identity_equal':b['identity']==v['identity'],'player_equal':b['player_id']==v['player_id'],'clock_equal':b['clock']==v['clock'],'complete_save_equal':b==v}
  assert b['clock']['paused'] is True and all(checks.values()),checks
  d={'release':'0.16.0','status':'passed','completed_utc':now(),'scope':'Real paused F5 saves immediately before and after exact accepted-request Review. All authoritative state, clock, identity and principal are compared; private save contents are not distributed.','confirmation':confirmation(process,capture),**checks,'before_save_sha256':sha(before),'after_save_sha256':sha(after)};write(output,d)
 else:
  baseline,exact,output,process,capture=a.inputs;b=read(baseline);e=read(exact);data=Path('/tmp/terra-native/launcher-review-fixture-016/player-data')
  assert e['status']=='passed' and len(b)==e['existing_fixture_files'];assert all(sha(data/name)==digest for name,digest in b.items())
  d={'release':'0.16.0','status':'passed','completed_utc':now(),'scope':'Read-only comparison of every file retained before named restore, after final recovered world closed normally. New recovered world may save its own continuation.','confirmation':confirmation(process,capture,'after_exit'),'all_original_files_unchanged':True,'file_count':len(b),'recovered_profile':e['profile'],'baseline_sha256':sha(baseline)};d['completed_utc']=now();write(output,d)
if __name__=='__main__':main()
