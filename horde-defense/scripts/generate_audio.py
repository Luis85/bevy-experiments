"""Generate Lastlight's original mono PCM game audio. No input assets required.

Owned files: ambience.wav in assets/audio (the shipped base ambience bed) and
eight legacy one-shot clips (shot, reload, melee, damage, boss, incoming, ready,
structure) in archive/audio (archived, not shipped, by production-pass
decision 10). Their manifests, assets/audio/PROVENANCE.json and
archive/audio/PROVENANCE.json, are written by
scripts/generate_atmosphere_audio.py; this script only reads them.

--check (the default) renders into a temporary directory outside the
workspace, compares each clip's SHA-256 with the committed file and with its
manifest record, writes nothing and exits 1 on any mismatch. --write
regenerates the nine clips in place. Run it as
python -I -B scripts/generate_audio.py [--check | --write].
"""
import argparse
import hashlib
import json
import math
import pathlib
import random
import struct
import sys
import tempfile
import wave

WORKSPACE=pathlib.Path(__file__).resolve().parents[1]
ASSETS=WORKSPACE/'assets'/'audio'
ARCHIVE=WORKSPACE/'archive'/'audio'
RATE=22050
TAU=math.tau

def make(name,seconds,fn,folder,seed=12):
    rng=random.Random(seed)
    samples=[]
    low=0.0
    for i in range(round(seconds*RATE)):
        t=i/RATE
        noise=rng.uniform(-1,1)
        low=0.94*low+0.06*noise
        value=fn(t,noise,low)
        # Small edge ramps keep separate sounds free of digital clicks.
        edge=min(1.0,t/0.002,(seconds-t)/0.015)
        samples.append(max(-0.95,min(0.95,value*max(0,edge))))
    payload=b''.join(struct.pack('<h',round(v*32767)) for v in samples)
    with wave.open(str(folder/f'{name}.wav'),'wb') as w:
        w.setnchannels(1);w.setsampwidth(2);w.setframerate(RATE);w.writeframes(payload)
    print(name,len(samples),len(payload))

def shot(t,n,low):
    crack=n*math.exp(-t*85)*0.7
    body=math.sin(TAU*(130*t-200*t*t))*math.exp(-t*25)*0.45
    tail=(n-low)*math.exp(-t*27)*0.12
    return crack+body+tail

def reload(t,n,low):
    value=0
    for onset,gain,pitch in [(0.025,0.34,1250),(0.20,0.18,1700),(0.48,0.42,760)]:
        d=t-onset
        if d>=0:value+=(n*0.65+math.sin(TAU*pitch*d)*0.35)*math.exp(-d*100)*gain
    if 0.24<t<0.45:value+=(n-low)*0.09*math.sin(math.pi*(t-0.24)/0.21)
    return value

def melee(t,n,low):
    swish=(n-low)*math.sin(math.pi*min(1,t/0.28))**2*0.18
    impact=0
    if t>0.19:
        d=t-0.19;impact=(math.sin(TAU*(105*d-180*d*d))*0.5+n*0.22)*math.exp(-d*40)
    return swish+impact

def damage(t,n,low):
    return (math.sin(TAU*(70*t-35*t*t))*0.5+low*0.55)*math.exp(-t*12)

def boss(t,n,low):
    envelope=min(1,t/0.18)*min(1,(1.6-t)/0.55)
    pulse=0.8+0.2*math.sin(TAU*3*t)
    horn=math.sin(TAU*73.42*t)*0.17+math.sin(TAU*110*t)*0.11+math.sin(TAU*146.84*t)*0.06
    return horn*envelope*pulse+low*0.07*envelope

def ambience(t,n,low):
    # Every oscillator completes an integer number of cycles in this 8s loop.
    env=min(1,t/0.25,(8-t)/0.25)
    wind=low*0.11*(0.7+0.3*math.sin(TAU*0.125*t))
    hum=math.sin(TAU*48*t)*0.012+math.sin(TAU*72*t)*0.006
    distant=math.sin(TAU*137.5*t)*0.002*(1+math.sin(TAU*0.25*t))
    return (wind+hum+distant)*max(0,env)


# Additional tactical cues: radio arrival ticks, a rising readiness chime,
# and a low mechanical collapse. Each has a different rhythm and spectrum.
def incoming(t,n,low):
    value=0.0
    for onset,pitch in [(0.025,820),(0.16,1120)]:
        d=t-onset
        if d>=0:
            value+=(math.sin(TAU*(pitch*d-950*d*d))*0.22+(n-low)*0.055)*math.exp(-d*42)
    return value


def ready(t,n,low):
    value=0.0
    for onset,pitch,gain in [(0.015,660,0.16),(0.13,990,0.21)]:
        d=t-onset
        if d>=0:
            value+=(math.sin(TAU*pitch*d)+0.23*math.sin(TAU*pitch*2*d))*gain*math.exp(-d*21)
    return value


def structure(t,n,low):
    value=0.0
    for onset,gain,pitch in [(0.015,0.60,95),(0.15,0.36,68),(0.33,0.24,42)]:
        d=t-onset
        if d>=0:
            value+=(math.sin(TAU*(pitch*d-24*d*d))*0.52+low*0.85+(n-low)*0.18)*gain*math.exp(-d*14)
    return value


# The owned clips: name, seconds, synthesis and the folder each one lives in.
CLIPS=[
    ('shot',0.22,shot,ARCHIVE),
    ('reload',0.68,reload,ARCHIVE),
    ('melee',0.44,melee,ARCHIVE),
    ('damage',0.40,damage,ARCHIVE),
    ('boss',1.60,boss,ARCHIVE),
    ('ambience',8.0,ambience,ASSETS),
    ('incoming',0.34,incoming,ARCHIVE),
    ('ready',0.36,ready,ARCHIVE),
    ('structure',0.78,structure,ARCHIVE),
]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def manifest_sha256(folder,name):
    path=folder/'PROVENANCE.json'
    if not path.is_file():
        return None
    files=json.loads(path.read_text(encoding='utf-8')).get('files',[])
    return next((entry.get('sha256') for entry in files if entry.get('file')==f'{name}.wav'),None)

def check():
    failures=[]
    with tempfile.TemporaryDirectory(prefix='lastlight-audio-') as temp:
        temp=pathlib.Path(temp)
        for name,seconds,fn,folder in CLIPS:
            make(name,seconds,fn,temp)
            rendered=digest(temp/f'{name}.wav')
            path=folder/f'{name}.wav'
            where=path.relative_to(WORKSPACE)
            if not path.is_file():
                failures.append(f'{where}: missing')
            elif digest(path)!=rendered:
                failures.append(f'{where}: rendered sha256 differs')
            if manifest_sha256(folder,name)!=rendered:
                failures.append(f'{where}: {folder.relative_to(WORKSPACE)}/PROVENANCE.json sha256 differs')
    for failure in failures:
        print('FAIL: '+failure)
    if failures:
        print('Run with --write, then generate_atmosphere_audio.py --write, and review the diff.')
        return 1
    print(f'PASS: {len(CLIPS)} clips are byte-identical to a fresh render and to their manifests; nothing written.')
    return 0

def write():
    for name,seconds,fn,folder in CLIPS:
        folder.mkdir(parents=True,exist_ok=True)
        make(name,seconds,fn,folder)
    print('Wrote the nine clips; run generate_atmosphere_audio.py --write to refresh the manifests.')
    return 0

def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode=parser.add_mutually_exclusive_group()
    mode.add_argument('--check',action='store_true',help='render to a temp dir and compare (default)')
    mode.add_argument('--write',action='store_true',help='regenerate the nine clips in place')
    arguments=parser.parse_args(argv)
    return write() if arguments.write else check()

if __name__=='__main__':
    sys.exit(main())
