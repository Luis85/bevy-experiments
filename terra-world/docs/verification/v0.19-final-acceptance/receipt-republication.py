from pathlib import Path
from collections import Counter
from datetime import datetime, timezone
import hashlib,json,os,re,subprocess,sys
sys.dont_write_bytecode=True
ROOT=Path('/workspace/scratch/1c0c5a298a25/project/terra-world')
DIRECTORY=ROOT/'docs/verification/v0.19-final-acceptance'
sys.path.insert(0,str(ROOT/'scripts'))
from verify import source_inputs,test_identities,write_json
from release_contract import validate_gate_results,RUST_TEST_GATE_NAMES

def sha(data):return hashlib.sha256(data).hexdigest()
raw=(DIRECTORY/'gates.json').read_bytes()
report=json.loads(raw)
assert report['status']=='running' and len(report['gates'])==44
validate_gate_results(report['gates'])
metadata=json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version','1','--locked','--offline'],cwd=ROOT,text=True))
packages=metadata['packages']
workspace={p['name'] for p in packages if p['id'] in metadata['workspace_members']}
identities=set()
for gate in report['gates']:
 body=(DIRECTORY/gate['log']).read_bytes()
 assert sha(body)==gate['log_sha256'],gate['name']
 summaries=[tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',body.decode())]
 assert sum(x[0] for x in summaries)==gate['passed_test_executions'],gate['name']
 assert sum(x[1] for x in summaries)==gate['failed_tests']==0,gate['name']
 assert sum(x[2] for x in summaries)==gate['ignored_tests']==0,gate['name']
 if gate['name'] in RUST_TEST_GATE_NAMES:
  if gate['name']=='headless':selected=workspace-{'terra-demo','terra-bevy'}
  elif gate['name'] in {'bevy-headless','bevy-render'}:selected={'terra-bevy'}
  elif gate['name'] in {'demo','launcher-recovery','launcher-world-names'}:selected={'terra-demo'}
  elif gate['name']=='session-probe-checks':selected={'terra-net'}
  else:selected={'terra-sandbox'}
  found=test_identities(body.decode(),packages,selected)
  assert len(found)==gate['passed_test_executions'],gate['name']
  identities.update(found)
for name,expected in report['evidence_artifacts'].items():assert sha((DIRECTORY/name).read_bytes())==expected,name
frozen=json.loads((DIRECTORY/'source-hashes.json').read_text())
assert source_inputs()==frozen and len(frozen)==report['source_inputs']==369
stored=json.loads((DIRECTORY/'distinct-tests.json').read_text())
expected=[{'crate':c,'harness':h,'name':n} for c,h,n in sorted(identities)]
assert stored==expected and len(identities)==946
native_log=(DIRECTORY/'native-build.log').read_text()
artifacts=[]
for line in native_log.splitlines():
 if line.startswith('{'):
  item=json.loads(line)
  if item.get('reason')=='compiler-artifact' and item.get('target',{}).get('name')=='terra-demo' and item.get('executable'):artifacts.append(item)
assert len(artifacts)==1
binary=Path(artifacts[0]['executable'])
binary_sha=sha(binary.read_bytes())
assert binary_sha=='b86ad4bc44685780b55c6f3eb8f32f88af4330f6f055d9f7d8d71e54339fe704'
snapshot=DIRECTORY/'runner-progress-snapshot.json'
assert not snapshot.exists()
snapshot.write_bytes(raw)
report['evidence_artifacts']['runner-progress-snapshot.json']=sha(raw)
report['evidence_artifacts']['distinct-tests.json']=sha((DIRECTORY/'distinct-tests.json').read_bytes())
report['source_inputs_unchanged']=True
report['published_reports_unchanged']=all(sha((DIRECTORY/n).read_bytes())==h for n,h in report['evidence_artifacts'].items())
report['published_logs_unchanged']=all(sha((DIRECTORY/g['log']).read_bytes())==g['log_sha256'] for g in report['gates'])
report['distinct_tests_by_crate']=dict(sorted(Counter(item[0] for item in identities).items()))
report['distinct_passing_rust_tests']=len(identities)
report['passing_rust_test_executions']=sum(g['passed_test_executions'] for g in report['gates'])
report['native_executable']=str(binary)
report['native_binary_sha256']=binary_sha
report['status']='passed'
report['status_origin']='independently verified receipt reconstruction after final runner snapshot was not retained'
report['republication']={
 'published_utc':datetime.now(timezone.utc).isoformat(),
 'original_runner_finish_utc':None,
 'original_runner_finish_time_not_reconstructed':True,
 'original_snapshot_sha256':sha(raw),
 'original_snapshot':'runner-progress-snapshot.json',
 'reported_process_exit_code':0,
 'reported_terminal_summary':'Verification passed: 946 distinct Rust tests.',
 'process_outcome_provenance':'Integrator observed command-session completion output; distinct from independent file checks.',
 'independent_checks':['Complete strict 44-gate roster and typed counters','All gate log hashes','All captured Rust test summary counters','Every Rust-test identity reparsed from logs with Cargo metadata','Exact 946-identity manifest equality','Every recorded artifact hash','All 369 source hashes','One native compiler artifact and exact executable digest'],
 'cause':'Final persisted record retained running/44 despite observed completed exit0; root cause unestablished. No tests were rerun or newly invented in this reconstruction.',
 'native_visual_qualification':False,
}
assert report['published_reports_unchanged'] and report['published_logs_unchanged']
output=DIRECTORY/'accepted-gates.json'
assert not output.exists()
write_json(output,report)
assert json.loads(output.read_text())==report
print(json.dumps({'status':report['status'],'record':str(output),'distinct':len(identities),'executions':report['passing_rust_test_executions'],'native_binary_sha256':binary_sha,'original_snapshot_unchanged':(DIRECTORY/'gates.json').read_bytes()==raw},indent=2))
