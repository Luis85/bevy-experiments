#!/usr/bin/env python3
"""Explicit native UI actions, each followed by a fresh full-display capture."""
import argparse,json,os,subprocess,time
from pathlib import Path
from PIL import ImageGrab
os.environ['DISPLAY']=':101'
def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True).strip()
def capture(name):
    p=Path('/tmp/terra-native')/(name+'.png');assert not p.exists(),p
    window=xdo('search','--onlyvisible','--name','Terra').splitlines()[-1]
    geo=dict(line.split('=',1) for line in xdo('getwindowgeometry','--shell',window).splitlines() if '=' in line)
    x,y,w,h=[int(geo[k]) for k in ('X','Y','WIDTH','HEIGHT')]
    ImageGrab.grab(bbox=(x,y,x+w,y+h),xdisplay=':101').save(p);print(json.dumps({'capture':str(p),'window_pixels':[w,h]}))
def main():
    p=argparse.ArgumentParser();p.add_argument('--hold',type=float,default=.65);p.add_argument('action',choices=['state','capture','click','key','type','resize','scroll','drag','look','holdkey','shortcut','quitclick','close']);p.add_argument('values',nargs='*');a=p.parse_args()
    if a.action not in ['state','close','quitclick']:
        assert a.values and not (Path('/tmp/terra-native')/(a.values[-1]+'.png')).exists(), 'Capture name must be new before any input'
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
