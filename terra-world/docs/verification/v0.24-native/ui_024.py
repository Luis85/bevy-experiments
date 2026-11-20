#!/usr/bin/env python3
"""Explicit native UI actions, each followed by a fresh full-display capture."""
import argparse,hashlib,json,os,subprocess,time,tempfile
from datetime import datetime,timezone
from pathlib import Path
from PIL import ImageGrab
os.environ['DISPLAY']=':105'
def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True,stderr=subprocess.PIPE).strip()
def now():return datetime.now(timezone.utc).isoformat()
def capture_paths(name):
    if not name or Path(name).name!=name or name in ('.','..'):
        raise ValueError('Capture name must be one filename stem')
    image=Path('/workspace/scratch/96f1d170dd13/source/terra-world/docs/verification/v0.24-native')/(name+'.png')
    receipt=Path('/workspace/scratch/96f1d170dd13/source/terra-world/docs/verification/v0.24-native')/(name+'-capture.json')
    for path in (image,receipt):
        if path.exists() or path.is_symlink():raise FileExistsError(path)
    return image,receipt
def window_pid(window):
    try:
        raw=xdo('getwindowpid',window)
        pid=int(raw)
        if pid<=0:raise ValueError('Window PID is not positive')
        return pid,None
    except (subprocess.CalledProcessError,ValueError) as error:
        detail=getattr(error,'stderr',None) or str(error)
        return None,str(detail).strip()[:1000]
def capture(name):
    p,receipt=capture_paths(name)
    started=now()
    window=xdo('search','--onlyvisible','--name','Terra').splitlines()[-1]
    title=xdo('getwindowname',window)
    pid,pid_error=window_pid(window)
    geo=dict(line.split('=',1) for line in xdo('getwindowgeometry','--shell',window).splitlines() if '=' in line)
    x,y,w,h=[int(geo[k]) for k in ('X','Y','WIDTH','HEIGHT')]
    frame=ImageGrab.grab(bbox=(x,y,x+w,y+h),xdisplay=':105')
    if frame.size!=(w,h):raise ValueError('Capture pixels differ from window geometry')
    with tempfile.NamedTemporaryFile(dir=p.parent,prefix='.capture-',suffix='.png',delete=False) as output:
        temporary=Path(output.name)
        frame.save(output,format='PNG');output.flush();os.fsync(output.fileno())
    try:
        os.link(temporary,p)
    finally:
        temporary.unlink()
    after_pid,after_error=window_pid(window)
    if pid is None or after_pid is None:
        pid_status='unavailable';verified_pid=None
    elif pid!=after_pid:
        pid_status='changed';verified_pid=None
    else:
        pid_status='available';verified_pid=pid
    with p.open('rb') as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
    record={
        'schema':1,'release':'0.24.0','capture_name':name,
        'capture_started_utc':started,'captured_utc':now(),
        'window_id':window,'window_title':title,'window_pid':verified_pid,
        'window_pid_status':pid_status,'window_pid_before':pid,'window_pid_after':after_pid,
        'window_pid_error':pid_error or after_error,
        'window_position':[x,y],'window_pixels':[w,h],
        'image':{'name':p.name,'sha256':digest,'bytes':p.stat().st_size},
        'association_scope':'Window PID read before and after capture; unsupported or changed properties do not assert a process association.',
    }
    with receipt.open('x') as output:json.dump(record,output,indent=2);output.write('\n')
    print(json.dumps({'capture':str(p),'receipt':str(receipt),'window_pixels':[w,h],'window_pid_status':pid_status,'window_pid':verified_pid}))
def main():
    p=argparse.ArgumentParser();p.add_argument('--hold',type=float,default=.65);p.add_argument('action',choices=['state','capture','click','key','type','resize','scroll','drag','look','holdkey','shortcut','quitclick','close']);p.add_argument('values',nargs='*');a=p.parse_args()
    if a.action not in ['state','close','quitclick']:
        if not a.values:raise ValueError('A fresh capture name is required before input')
        capture_paths(a.values[-1])
    ids=xdo('search','--onlyvisible','--name','Terra').splitlines()
    if a.action=='state':
        print(json.dumps([{'window':i,'title':xdo('getwindowname',i),'geometry':xdo('getwindowgeometry','--shell',i)} for i in ids],indent=2));return
    assert ids,'No visible Terra window';window=ids[-1];xdo('windowfocus','--sync',window)
    if a.action=='capture':capture(a.values[0]);return
    if a.action=='shortcut':
        keys=a.values[0].split('+')
        for key in keys:xdo('keydown',key);time.sleep(.12)
        for key in reversed(keys):xdo('keyup',key);time.sleep(.12)
        time.sleep(2);capture(a.values[1]);return
    if a.action=='holdkey':
        xdo('keydown',a.values[0])
        try:
            time.sleep(a.hold);capture(a.values[1])
        finally:xdo('keyup',a.values[0])
        return
    if a.action=='look':
        xdo('mousemove','--sync',a.values[0],a.values[1]);time.sleep(.3);xdo('mousedown','3');time.sleep(.4);xdo('mousemove_relative','--sync','--',a.values[2],a.values[3])
        try:
            time.sleep(2);capture(a.values[4])
        finally:xdo('mouseup','3')
        return
    if a.action in ['click','quitclick']:
        xdo('mousemove','--sync',a.values[0],a.values[1]);time.sleep(.25);xdo('mousedown','1');time.sleep(a.hold);xdo('mouseup','1')
    elif a.action=='key':
        xdo('keydown',a.values[0]);time.sleep(a.hold);xdo('keyup',a.values[0])
    elif a.action=='type':xdo('type','--clearmodifiers','--delay','80',a.values[0])
    elif a.action=='resize':xdo('windowsize',window,a.values[0],a.values[1])
    elif a.action=='scroll':
        xdo('mousemove','--sync',a.values[0],a.values[1]);time.sleep(.2);count=int(a.values[2]);xdo('click','--repeat',abs(count),'--delay',120,'5' if count>0 else '4')
    elif a.action=='drag':
        xdo('mousemove','--sync',a.values[0],a.values[1]);xdo('mousedown',a.values[4]);time.sleep(.2);xdo('mousemove','--sync',a.values[2],a.values[3]);time.sleep(a.hold);xdo('mouseup',a.values[4])
    elif a.action=='close':xdo('windowclose',window)
    time.sleep(3)
    if a.values and a.action not in ['close','quitclick']:capture(a.values[-1])
if __name__=='__main__':main()
