#!/usr/bin/env python3
"""Deterministically author Terra's original animated wildlife GLBs (stdlib only).
The pack and generator are MIT OR Apache-2.0, like the source distribution.
Units are metres; +Y is up, -Z is forward, and the origin is on the ground.
"""
import hashlib
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent

def quat_x(angle):
    return [math.sin(angle / 2), 0, 0, math.cos(angle / 2)]

def quat_y(angle):
    return [0, math.sin(angle / 2), 0, math.cos(angle / 2)]

class Model:
    def __init__(self):
        self.bin = bytearray()
        self.g = {'asset': {'version':'2.0', 'generator':'Terra original wildlife authoring pipeline'},
                  'scene':0, 'scenes':[{'name':'Terra wildlife', 'nodes':[0]}],
                  'nodes':[{'name':'animal', 'children':[]}], 'meshes':[], 'materials':[],
                  'buffers':[], 'bufferViews':[], 'accessors':[], 'animations':[]}
        self.materials = {}

    def accessor(self, values, width, component=5126, bounds=False):
        while len(self.bin) % 4: self.bin.append(0)
        start = len(self.bin)
        flat = [n for row in values for n in (row if isinstance(row, (tuple,list)) else [row])]
        code = {5126:'f',5123:'H',5125:'I'}[component]
        self.bin += struct.pack('<'+code*len(flat), *flat)
        view = len(self.g['bufferViews'])
        self.g['bufferViews'].append({'buffer':0, 'byteOffset':start, 'byteLength':len(self.bin)-start})
        item = {'bufferView':view, 'componentType':component, 'count':len(values), 'type':{1:'SCALAR',2:'VEC2',3:'VEC3',4:'VEC4'}[width]}
        if bounds:
            rows = values if width > 1 else [[v] for v in values]
            item['min'] = [min(row[i] for row in rows) for i in range(width)]
            item['max'] = [max(row[i] for row in rows) for i in range(width)]
        self.g['accessors'].append(item)
        return len(self.g['accessors'])-1

    def node(self, name, position=(0,0,0), parent=0):
        idx = len(self.g['nodes'])
        self.g['nodes'].append({'name':name, 'translation':list(position), 'children':[]})
        self.g['nodes'][parent]['children'].append(idx)
        return idx

    def material(self, color):
        if color not in self.materials:
            idx = len(self.g['materials'])
            self.g['materials'].append({'name':f'earth-tone-{idx}', 'pbrMetallicRoughness':{'baseColorFactor':list(color)+[1], 'metallicFactor':0, 'roughnessFactor':0.86}})
            self.materials[color] = idx
        return self.materials[color]

    def ellipsoid(self, name, center, radii, color, parent=0, rings=5, sides=10):
        points=[]
        for j in range(rings+1):
            theta=math.pi*j/rings
            for k in range(sides):
                phi=2*math.pi*k/sides
                points.append((center[0]+radii[0]*math.sin(theta)*math.cos(phi), center[1]+radii[1]*math.cos(theta), center[2]+radii[2]*math.sin(theta)*math.sin(phi)))
        vertices=[]; normals=[]
        for j in range(rings):
            for k in range(sides):
                a=j*sides+k; b=j*sides+(k+1)%sides; c=(j+1)*sides+k; d=(j+1)*sides+(k+1)%sides
                for tri in ((a,b,c),(b,d,c)):
                    p,q,r=[points[i] for i in tri]
                    u=[q[i]-p[i] for i in range(3)]; v=[r[i]-p[i] for i in range(3)]
                    n=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]]
                    length=math.sqrt(sum(a*a for a in n))
                    if length<1e-9: continue
                    n=[a/length for a in n]
                    # Cross-product outward winding: back-face culling stays enabled.
                    vertices.extend((p,q,r)); normals.extend((n,n,n))
        pos=self.accessor(vertices,3,bounds=True); normal=self.accessor(normals,3)
        mesh=len(self.g['meshes'])
        self.g['meshes'].append({'name':name+'-geometry','primitives':[{'attributes':{'POSITION':pos,'NORMAL':normal}, 'material':self.material(color)}]})
        node=self.node(name,parent=parent)
        self.g['nodes'][node]['mesh']=mesh
        return node

    def animation(self, name, duration, tracks):
        times=[duration*i/16 for i in range(17)]
        input_idx=self.accessor(times,1,bounds=True)
        samplers=[]; channels=[]
        for node,path,func in tracks:
            values=[func(i/16) for i in range(17)]
            output=self.accessor(values,4 if path=='rotation' else 3)
            channels.append({'sampler':len(samplers),'target':{'node':node,'path':path}})
            samplers.append({'input':input_idx,'output':output,'interpolation':'LINEAR'})
        self.g['animations'].append({'name':name,'samplers':samplers,'channels':channels})

    def save(self,path):
        self.g['buffers']=[{'byteLength':len(self.bin)}]
        for node in self.g['nodes']:
            if not node.get('children'): node.pop('children',None)
        encoded=json.dumps(self.g, separators=(',',':'),allow_nan=False).encode()
        encoded+=b' '*((-len(encoded))%4)
        data=bytes(self.bin)+b'\0'*((-len(self.bin))%4)
        glb=struct.pack('<4sII',b'glTF',2,12+8+len(encoded)+8+len(data))+struct.pack('<I4s',len(encoded),b'JSON')+encoded+struct.pack('<I4s',len(data),b'BIN\0')+data
        path.write_bytes(glb)
        return hashlib.sha256(glb).hexdigest()

SPECS={
'deer':dict(height=1.2,width=.32,length=.68,leg=.88,head=.27,coat=(.44,.26,.13),muzzle=(.20,.13,.09),ears=.22,horns=True),
'ibex':dict(height=1.04,width=.32,length=.58,leg=.72,head=.25,coat=(.39,.35,.27),muzzle=(.19,.18,.15),ears=.15,horns=True),
'boar':dict(height=.63,width=.40,length=.63,leg=.40,head=.27,coat=(.27,.24,.21),muzzle=(.19,.17,.15),ears=.13,horns=False),
'fox':dict(height=.48,width=.20,length=.43,leg=.33,head=.18,coat=(.69,.32,.12),muzzle=(.85,.76,.58),ears=.17,horns=False),
'hare':dict(height=.30,width=.18,length=.25,leg=.18,head=.13,coat=(.47,.39,.28),muzzle=(.76,.69,.55),ears=.31,horns=False),
}

def build(species,s):
    m=Model(); h=s['height']; w=s['width']; length=s['length']; leg=h-.05-(.045 if species=='hare' else .08); head_size=s['head']; coat=s['coat']; dark=s['muzzle']
    body=m.node('body',(0,h,0))
    m.ellipsoid('torso',(0,0,0),(w,.35*length+.08,length),coat,body,rings=6,sides=12)
    m.ellipsoid('chest',(0,.04,-length*.5),(w*.92,.34*length+.1,length*.5),coat,body)
    head=m.node('head',(0,.19 if species in ('deer','ibex') else .04,-length*.85),body)
    if species in ('deer','ibex'):
        m.ellipsoid('neck',(0,.01,.03),(w*.61,.33,.24),coat,head)
        skull_y=.27
    else: skull_y=.07
    m.ellipsoid('skull',(0,skull_y,-.14),(head_size*.72,head_size*.8,head_size),coat,head)
    m.ellipsoid('muzzle',(0,skull_y-.08,-head_size-.12),(head_size*.55,head_size*.38,head_size*.64),dark,head)
    m.ellipsoid('nose',(0,skull_y-.07,-head_size*1.53-.13),(head_size*.28,head_size*.2,.035),(.095,.085,.07),head)
    for side in (-1,1):
        m.ellipsoid('eye-'+str(side),(side*head_size*.65,skull_y+.025,-.20),(.027,.027,.028),(.035,.032,.025),head,rings=4,sides=8)
        m.ellipsoid('ear-'+str(side),(side*head_size*.65,skull_y+head_size*.75+s['ears']*.5,-.08),(head_size*.26,s['ears']*.65,head_size*.28),coat,head)
        m.ellipsoid('ear-inner-'+str(side),(side*head_size*.65,skull_y+head_size*.75+s['ears']*.5,-.08-head_size*.23),(head_size*.13,s['ears']*.43,.015),dark,head)
        if s['horns']:
            for segment in range(3):
                m.ellipsoid(f'horn-{side}-{segment}',(side*(head_size*.40+segment*.055),skull_y+head_size*.7+.15+segment*.15,-.02+segment*.09),(.037,.16,.04),(.33,.28,.19),head,rings=4,sides=7)
            if species=='deer':
                m.ellipsoid('antler-tine-'+str(side),(side*.28,skull_y+.55,-.13),(.032,.19,.035),(.36,.29,.18),head,rings=4,sides=7)
        if species=='boar':
            m.ellipsoid('tusk-'+str(side),(side*.18,skull_y-.04,-.46),(.035,.10,.035),(.83,.80,.64),head,rings=4,sides=7)
    legs=[]
    for front in (True,False):
        for side in (-1,1):
            name=('front' if front else 'rear')+('_left' if side<0 else '_right')
            pivot=m.node(name,(side*w*.64,h-.05,(-.57 if front else .55)*length))
            legs.append(pivot)
            m.ellipsoid(name+'-limb',(0,-leg*.47,0),(w*.23,leg*.53,w*.25),coat,pivot,rings=5,sides=8)
            m.ellipsoid(name+'-foot',(0,-leg*.97,-.04),(w*.24,.08 if species!='hare' else .045,.11 if species!='hare' else .08),dark,pivot,rings=4,sides=8)
    tail=m.node('tail',(0,h+.03,length*.85))
    tail_length=.46 if species=='fox' else .13
    m.ellipsoid('tail-fur',(0,-.04,tail_length*.55),(w*.37,w*.38,tail_length),coat,tail)
    if species=='fox': m.ellipsoid('tail-tip',(0,-.04,tail_length*1.32),(w*.3,w*.3,tail_length*.3),(.86,.81,.68),tail)
    elif species=='hare': m.ellipsoid('tail-light',(0,0,.11),(.07,.07,.09),(.81,.77,.65),tail)
    base_y=h
    for clip,duration,amplitude in [('idle',3.0,.0),('walk',1.15,.34),('run',.68,.63),('feed',2.6,0),('drink',2.8,0)]:
        tracks=[]
        for i,node in enumerate(legs):
            phase=0 if i in (0,3) else math.pi
            if clip=='run' and species=='hare': phase=0 if i<2 else math.pi
            tracks.append((node,'rotation',lambda t,p=phase,a=amplitude:quat_x(math.sin(t*math.tau+p)*a)))
        bob=.016 if clip=='idle' else (.028 if clip=='walk' else .045 if clip=='run' else .01)
        tracks.append((body,'translation',lambda t,b=bob:[0,base_y+b*math.sin(t*math.tau*(2 if amplitude else 1)),0]))
        pitch=-.73 if clip=='feed' else -.95 if clip=='drink' else -.03
        tracks.append((head,'rotation',lambda t,p=pitch:quat_x(p+.035*math.sin(t*math.tau))))
        tracks.append((tail,'rotation',lambda t:quat_y(.15*math.sin(t*math.tau))))
        m.animation(clip,duration,tracks)
    digest=m.save(ROOT/(species+'.glb'))
    return {'id':species,'file':species+'.glb','sha256':digest,'scene':0}

models=[build(species,s) for species,s in SPECS.items()]
manifest={'schema_version':1,'name':'Terra woodland wildlife','author':'Terra World contributors','license':'MIT OR Apache-2.0','models':models,
          'bindings':[{'target':{'type':'species','value':species},'model':species,'scale':1,'ground_offset_m':0,'yaw_offset_degrees':0,
                       'animation':{'idle':'idle','walk':'walk','run':'run','feed':'feed','drink':'drink','walk_speed_m_s':1.4 if species!='hare' else .9,'run_speed_m_s':4.4 if species!='hare' else 3.8,'blend_seconds':.22}}
                      for species in SPECS]}
(ROOT/'pack.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Generated',len(models),'original models; bytes',sum((ROOT/m['file']).stat().st_size for m in models))
