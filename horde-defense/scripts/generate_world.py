"""Rebuild the compact Lastlight world catalogue from public-domain Natural Earth.
Usage: python scripts/generate_world.py [110m.geojson 50m.geojson]
Without arguments downloads the source files into the OS temporary directory.
No third-party Python modules are required. Never estimates present-day population.
"""
import collections
import hashlib
import json
import math
import pathlib
import sys
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = 'https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/'
NAMES = ['ne_110m_admin_0_countries.geojson', 'ne_50m_admin_0_countries.geojson']
paths = [pathlib.Path(p) for p in sys.argv[1:]]
if not paths:
    paths = [pathlib.Path(tempfile.gettempdir()) / n for n in NAMES]
    for path, name in zip(paths, NAMES):
        urllib.request.urlretrieve(BASE + name, path)
if len(paths) != 2:
    raise SystemExit('Supply both 110m and 50m GeoJSON files, or no arguments.')
raws = [p.read_bytes() for p in paths]
datasets = [json.loads(raw) for raw in raws]
NORMALIZE = {'PSX':'PSE','SDS':'SSD','KOS':'XKX','SAH':'ESH','ALD':'ALA'}
def key(feature):
    code = feature['properties']['ADM0_A3']
    return NORMALIZE.get(code, code)
def rings(feature):
    geometry=feature['geometry']
    polygons=geometry['coordinates'] if geometry['type']=='MultiPolygon' else [geometry['coordinates']]
    return [poly[0] for poly in polygons if poly and len(poly[0])>=4]
coarse = {key(f):f for f in datasets[0]['features']}
fine = {key(f):f for f in datasets[1]['features']}
neighbor = {k:set() for k in fine}
seas = {k:set() for k in fine}
edges=collections.defaultdict(set)
for code, feature in fine.items():
    for ring in rings(feature):
        pts=[(round(p[0],4),round(p[1],4)) for p in ring]
        for a,b in zip(pts,pts[1:]):
            if a!=b:
                edges[tuple(sorted([a,b]))].add(code)
for countries in edges.values():
    for a in countries:
        neighbor[a].update(countries-{a})
# Microstate borders can fall between the source's generalized vertices.
# These explicit real land borders also preserve the intuitive campaign links.
LAND = [('VAT','ITA'),('SMR','ITA'),('MCO','FRA'),('AND','FRA'),('AND','ESP'),('LIE','CHE'),('LIE','AUT'),('GIB','ESP'),('MAC','CHN'),('HKG','CHN'),('MAF','SXM')]
for a,b in LAND:
    if a in neighbor and b in neighbor:
        neighbor[a].add(b);neighbor[b].add(a)
centers={k:(f['properties']['LABEL_X'],f['properties']['LABEL_Y']) for k,f in fine.items()}
def distance(a,b):
    ax,ay=centers[a]; bx,by=centers[b]
    # Angular-distance comparison on a sphere, robust across the dateline.
    la,lb=math.radians(ay),math.radians(by)
    return 1-(math.sin(la)*math.sin(lb)+math.cos(la)*math.cos(lb)*math.cos(math.radians(ax-bx)))
def connect(a,b):
    neighbor[a].add(b);neighbor[b].add(a);seas[a].add(b);seas[b].add(a)
SEA=[('SGP','MYS'),('BHR','SAU'),('ALA','FIN'),('GBR','FRA'),('IRL','GBR'),('ISL','GBR'),('JPN','KOR'),('TWN','CHN'),('PHL','TWN'),('IDN','MYS'),('AUS','IDN'),('NZL','AUS'),('MDG','MOZ'),('CYP','GRC'),('MLT','ITA'),('CUB','USA'),('BHS','USA'),('LKA','IND'),('FJI','NZL'),('GRL','ISL')]
for a,b in SEA:
    if a in neighbor and b in neighbor:connect(a,b)
# Give every otherwise isolated island a visible sea route. Routes are gameplay
# connections, not a statement of political, migration or maritime jurisdiction.
for a in sorted(neighbor):
    if not neighbor[a]:
        b=min((k for k in neighbor if k!=a), key=lambda b:(distance(a,b),b))
        connect(a,b)
def component(start):
    seen={start}; todo=[start]
    while todo:
        a=todo.pop()
        for b in neighbor[a]-seen:seen.add(b);todo.append(b)
    return seen
while len(component('DEU'))<len(neighbor):
    seen=component('DEU'); outside=set(neighbor)-seen
    a,b=min(((a,b) for a in seen for b in outside),key=lambda pair:(distance(*pair),pair))
    connect(a,b)
ARID=set('DZA EGY LBY MAR TUN ESH MRT MLI NER TCD SDN SAU OMN ARE QAT KWT BHR IRQ JOR ISR PSE NAM BWA AUS MNG KAZ TKM UZB'.split())
ALPINE=set('NPL BTN CHE AUT AND LIE GEO ARM AFG KGZ TJK PER BOL LSO'.split())
TROPICAL_REGIONS={'Caribbean','Melanesia','Micronesia','Polynesia','South-Eastern Asia','Middle Africa','Western Africa'}
profiles={
    'Temperate':dict(enemy_health=1.0,enemy_speed=1.0,reward=1.0,supply=3,personal_damage=1.0,npc_skill=0),
    'Arid':dict(enemy_health=1.0,enemy_speed=1.10,reward=1.12,supply=-2,personal_damage=1.05,npc_skill=0),
    'Tropical':dict(enemy_health=0.95,enemy_speed=1.15,reward=1.10,supply=0,personal_damage=1.0,npc_skill=0),
    'Alpine':dict(enemy_health=1.15,enemy_speed=0.90,reward=1.18,supply=2,personal_damage=1.0,npc_skill=1),
    'Boreal':dict(enemy_health=1.10,enemy_speed=0.85,reward=1.10,supply=3,personal_damage=0.98,npc_skill=0),
}
words={'Temperate':'reclaimed rail yards','Arid':'sun-bleached service roads','Tropical':'overgrown market roads','Alpine':'mountain service passes','Boreal':'winter timber depots'}
countries=[]
for code,f in sorted(fine.items(),key=lambda kv:kv[1]['properties']['ADMIN']):
    p=f['properties'];lon,lat=centers[code]
    biome='Arid' if code in ARID else 'Alpine' if code in ALPINE else 'Boreal' if abs(lat)>54 else 'Tropical' if abs(lat)<23 or p['SUBREGION'] in TROPICAL_REGIONS else 'Temperate'
    mods=profiles[biome].copy()
    # A stable authored regional variation unrelated to economic or social rank.
    difficulty=round(0.94+(sum(code.encode())%15)/100,2)
    boosts=[f"Field supply {mods['supply']:+d}"] if mods['supply'] else []
    buffs=[];debuffs=[]
    if mods['enemy_speed']<1:buffs.append(f"Cold or terrain slows enemies {round((1-mods['enemy_speed'])*100)}%")
    if mods['enemy_speed']>1:debuffs.append(f"Fast approach: enemy speed +{round((mods['enemy_speed']-1)*100)}%")
    if mods['enemy_health']>1:debuffs.append(f"Durable hostiles: health +{round((mods['enemy_health']-1)*100)}%")
    if mods['enemy_health']<1:buffs.append(f"Weathered hostiles: health -{round((1-mods['enemy_health'])*100)}%")
    if mods['personal_damage']>1:buffs.append('Clear sightlines: personal damage +5%')
    if mods['personal_damage']<1:debuffs.append('Cold handling: personal damage -2%')
    if mods['npc_skill']:buffs.append('Elevated sightlines: NPC attack skill +1')
    if mods['reward']>1:boosts.append(f"Salvage value +{round((mods['reward']-1)*100)}%")
    # A distinct label and original local operation name for every region; avoid
    # suggesting these game conditions describe actual national characteristics.
    countries.append(dict(id=code,name=p.get('NAME_LONG') or p['ADMIN'],population=max(0,round(p['POP_EST'])),population_year=p['POP_YEAR'],polygons=[[[round(x,4),round(y,4)] for x,y in ring] for ring in rings(coarse.get(code,f))],center=[round(lon,4),round(lat,4)],neighbors=sorted(neighbor[code]),sea_neighbors=sorted(seas[code]),biome=biome,culture=f"{p['SUBREGION']} / {words[biome]}",region=p['SUBREGION'],kind=p['TYPE'],difficulty=difficulty,modifiers=mods,boosts=boosts,buffs=buffs,debuffs=debuffs,level_override=None))
out=dict(version=1,levels_per_population=10000000,min_levels=3,max_levels=24,strategy_config=dict(recruitment_unlock_freed=1,parallel_unlock_freed=2,strategy_unlock_freed=4,max_parallel=4,max_roster=12,recruit_cost=100,outbreak_interval_ticks=36000,outbreak_duration_ticks=25200,outbreak_chance_percent=35,guard_suppression_percent=30,max_guards_per_country=3),sources=[dict(url=BASE+name,sha256=hashlib.sha256(raw).hexdigest()) for name,raw in zip(NAMES,raws)],countries=countries)
path=ROOT/'crates/horde_core/src/world.json'
path.write_text(json.dumps(out,separators=(',',':'),ensure_ascii=False)+'\n',encoding='utf-8')
print(f"Generated {len(countries)} countries/territories; {sum(len(v) for v in neighbor.values())//2} links, {sum(len(v) for v in seas.values())//2} sea routes, {path.stat().st_size} bytes")
