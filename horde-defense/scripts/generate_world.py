"""Reproduce the Last Light world catalogue, crates/horde_core/src/world.json.

Only Python's standard library is used. Never estimates present-day population.

python -I -B scripts/generate_world.py [--from-existing]
    The default. Offline: loads the committed world.json, recomputes only each
    country's `difficulty` and writes the file back. Every other key, at the top
    level and in each country, is carried over unchanged.
python -I -B scripts/generate_world.py --check [--rebuild ...]
    Builds the same output but writes nothing. Exits 0 when it is byte-identical
    to the committed file and 1 when it differs.
python -I -B scripts/generate_world.py --rebuild (110m.geojson 50m.geojson | --download)
    Rebuilds the countries from public-domain Natural Earth. The source bytes must
    match the sha256 pins in world.json `sources` (exit 3, unless
    --allow-source-drift, which re-pins them) and no country's stage count may
    change (exit 4, unless --allow-stage-count-change). Every top-level key other
    than `sources` and `countries` (version, the level config, strategy_config,
    stage_themes) and each country's `level_override` are carried over.
python -I -B scripts/generate_world.py --self-test
    Exercises the modes above on throwaway files in a temporary directory.

--world PATH points any mode at another world.json. Exit codes: 0 success, 1 a
--check difference or a self-test failure, 2 usage, 3 source drift, 4 a stage-count
change. A refused run writes nothing.
"""
import argparse
import collections
import contextlib
import copy
import hashlib
import io
import json
import math
import pathlib
import sys
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
WORLD = ROOT / 'crates/horde_core/src/world.json'
BASE = 'https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/'
NAMES = ['ne_110m_admin_0_countries.geojson', 'ne_50m_admin_0_countries.geojson']
SOURCE_DRIFT = 3
STAGE_COUNT_CHANGE = 4


class Refused(Exception):
    """A --rebuild guard refused the run; `code` is the exit code."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def render(world):
    """The committed byte form of a world catalogue."""
    return (json.dumps(world, indent=2, ensure_ascii=False) + '\n').encode('utf-8')


FNV_OFFSET = 0xcbf29ce484222325
FNV_PRIME = 0x100000001b3
# Difficulty bands in hundredths by stage count (design-enemy-balance section 8,
# ruling W-10): up to 3 stages, 4 to 9, and 10 or more.
BANDS = ((3, 85, 115), (9, 100, 135), (None, 120, 160))


def fnv1a64(text):
    """FNV-1a 64 of the UTF-8 bytes of `text` (horde_core::hash::fnv1a)."""
    value = FNV_OFFSET
    for byte in text.encode('utf-8'):
        value = ((value ^ byte) * FNV_PRIME) & 0xffffffffffffffff
    return value


def band(stages):
    """The (lo, hi) difficulty band, in hundredths, for a country with `stages` stages."""
    for most, lo, hi in BANDS:
        if most is None or stages <= most:
            return lo, hi


def difficulty(country, world):
    """An authored game value from the country's size alone, never from national traits.

    The band follows the stage count, which population already sets; within the
    band a stable hash of the id picks the value, in whole hundredths:
    (lo + fnv1a64('difficulty:' + id) % (hi - lo + 1)) / 100. The game clamps the
    campaign's start country to [0.95, 1.05] (Country::effective_difficulty).
    """
    lo, hi = band(stage_count(country, world))
    return (lo + fnv1a64('difficulty:' + country['id']) % (hi - lo + 1)) / 100


def stage_count(country, world):
    """Country::level_count_with (world.rs): the override, else population per stage."""
    max_levels = world['max_levels']
    override = country.get('level_override')
    if override is not None:
        return min(max(override, 1), max(max_levels, 1))
    divisor = max(world['levels_per_population'], 1)
    count = -(-country['population'] // divisor)
    return min(max(count, world['min_levels']), max(max_levels, world['min_levels']))


def from_existing(world):
    """The committed catalogue with only `difficulty` recomputed."""
    out = copy.deepcopy(world)
    for country in out['countries']:
        country['difficulty'] = difficulty(country, out)
    return out


def build_countries(raws):
    """Countries from the 110m and 50m GeoJSON bytes, plus the link and sea-route counts."""
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
        countries.append(dict(id=code,name=p.get('NAME_LONG') or p['ADMIN'],population=max(0,round(p['POP_EST'])),population_year=p['POP_YEAR'],polygons=[[[round(x,4),round(y,4)] for x,y in ring] for ring in rings(coarse.get(code,f))],center=[round(lon,4),round(lat,4)],neighbors=sorted(neighbor[code]),sea_neighbors=sorted(seas[code]),biome=biome,culture=f"{p['SUBREGION']} / {words[biome]}",region=p['SUBREGION'],kind=p['TYPE'],difficulty=None,modifiers=mods,boosts=boosts,buffs=buffs,debuffs=debuffs,level_override=None))
    links = sum(len(v) for v in neighbor.values())//2
    routes = sum(len(v) for v in seas.values())//2
    return countries, links, routes


def rebuild(world, raws, allow_source_drift=False, allow_stage_count_change=False):
    """The catalogue rebuilt from source bytes, or Refused by the sha256 or stage-count guard."""
    sources = [dict(url=BASE + name, sha256=hashlib.sha256(raw).hexdigest()) for name, raw in zip(NAMES, raws)]
    pins = {source['url']: source['sha256'] for source in world.get('sources', [])}
    drift = [f"{source['url'].rsplit('/', 1)[-1]} is {source['sha256']}, world.json pins {pins.get(source['url'], 'nothing')}"
             for source in sources if pins.get(source['url']) != source['sha256']]
    if drift and not allow_source_drift:
        raise Refused(SOURCE_DRIFT, 'source sha256 drift: ' + '; '.join(drift) + ' (--allow-source-drift accepts and re-pins)')
    countries, links, routes = build_countries(raws)
    overrides = {country['id']: country.get('level_override') for country in world.get('countries', [])}
    for country in countries:
        country['level_override'] = overrides.get(country['id'])
    out = copy.deepcopy(world)
    out['sources'] = sources
    out['countries'] = countries
    for country in countries:
        country['difficulty'] = difficulty(country, out)
    before = {country['id']: stage_count(country, world) for country in world.get('countries', [])}
    after = {country['id']: stage_count(country, out) for country in countries}
    changed = [f"{code} {before.get(code, 'absent')} -> {after.get(code, 'absent')}"
               for code in sorted(set(before) | set(after)) if before.get(code) != after.get(code)]
    if changed and not allow_stage_count_change:
        shown = ', '.join(changed[:12]) + (f' and {len(changed) - 12} more' if len(changed) > 12 else '')
        raise Refused(STAGE_COUNT_CHANGE, f'{len(changed)} stage counts change: {shown} (--allow-stage-count-change accepts)')
    return out, links, routes


def download():
    raws = []
    for name in NAMES:
        with urllib.request.urlopen(BASE + name) as response:
            raws.append(response.read())
    return raws


def parser():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--from-existing', action='store_true', help='recompute difficulty in the committed catalogue (default)')
    mode.add_argument('--rebuild', action='store_true', help='rebuild the countries from Natural Earth GeoJSON')
    mode.add_argument('--self-test', action='store_true', help='exercise every mode on throwaway files')
    parser.add_argument('--check', action='store_true', help='write nothing; exit 1 if the output differs from the committed bytes')
    parser.add_argument('--download', action='store_true', help='with --rebuild, fetch both GeoJSON files')
    parser.add_argument('--allow-source-drift', action='store_true', help='with --rebuild, accept and re-pin changed source bytes')
    parser.add_argument('--allow-stage-count-change', action='store_true', help='with --rebuild, accept changed stage counts')
    parser.add_argument('--world', type=pathlib.Path, default=WORLD, help='the world.json to read and write (default: the committed one)')
    parser.add_argument('geojson', nargs='*', type=pathlib.Path, help='with --rebuild: the 110m and 50m GeoJSON files')
    return parser


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    rebuild_only = args.geojson or args.download or args.allow_source_drift or args.allow_stage_count_change
    if args.self_test:
        if args.check or rebuild_only or args.world != WORLD:
            cli.error('--self-test takes no other options')
        return self_test()
    if not args.rebuild and rebuild_only:
        cli.error('GeoJSON paths, --download and the --allow flags need --rebuild')
    if args.rebuild and args.download == bool(args.geojson):
        cli.error('--rebuild needs either the two GeoJSON paths or --download')
    if args.rebuild and args.geojson and len(args.geojson) != 2:
        cli.error('supply both the 110m and the 50m GeoJSON files')
    path = args.world
    committed = path.read_bytes()
    world = json.loads(committed.decode('utf-8'))
    summary = ''
    if args.rebuild:
        raws = download() if args.download else [geojson.read_bytes() for geojson in args.geojson]
        try:
            out, links, routes = rebuild(world, raws, args.allow_source_drift, args.allow_stage_count_change)
        except Refused as refused:
            print(f'FAIL: {refused}; {path} is unchanged')
            return refused.code
        summary = f'; {links} links, {routes} sea routes'
    else:
        out = from_existing(world)
    rendered = render(out)
    countries = len(out['countries'])
    if args.check:
        if rendered == committed:
            print(f'PASS: {path.name} is reproduced byte for byte ({countries} countries/territories, {len(rendered)} bytes)')
            return 0
        print(f'FAIL: the generated {path.name} differs from the committed bytes ({len(rendered)} vs {len(committed)} bytes); run without --check and review the diff')
        return 1
    path.write_bytes(rendered)
    print(f'Wrote {countries} countries/territories{summary}, {len(rendered)} bytes, to {path}')
    return 0


def self_test():
    results = []

    def case(name, condition, detail=''):
        results.append(bool(condition))
        print(('ok   ' if condition else 'FAIL ') + name + ('' if condition else f' - {detail}'))

    def run(*argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = main(list(argv))
            except SystemExit as exit:
                code = exit.code
        return code, out.getvalue() + err.getvalue()

    expected = b'{\n  "a": [\n    1,\n    {\n      "b": "\xc3\xa9"\n    }\n  ],\n  "c": 1.0\n}\n'
    got = render({'a': [1, {'b': 'é'}], 'c': 1.0})
    case('writer emits indent-2 UTF-8 with one trailing newline', got == expected, repr(got[:60]))

    case('fnv1a64 matches the FNV-1a 64 test vectors', fnv1a64('') == 0xcbf29ce484222325
         and fnv1a64('a') == 0xaf63dc4c8601ec8c and fnv1a64('foobar') == 0x85944171f73967e8)
    bands = {stages: band(stages) for stages in (1, 3, 4, 9, 10, 24)}
    case('the bands are 85-115 to 3 stages, 100-135 to 9, 120-160 from 10',
         bands == {1: (85, 115), 3: (85, 115), 4: (100, 135), 9: (100, 135), 10: (120, 160), 24: (120, 160)}, bands)

    committed = WORLD.read_bytes()
    committed_world = json.loads(committed.decode('utf-8'))
    examples = {country['id']: country['difficulty'] for country in committed_world['countries']
                if country['id'] in ('LUX', 'DEU', 'BEL', 'USA', 'CHN', 'AUT', 'BGD')}
    case('the committed difficulties are the design examples',
         examples == {'LUX': 0.92, 'DEU': 1.04, 'BEL': 0.97, 'USA': 1.27, 'CHN': 1.52, 'AUT': 0.85, 'BGD': 1.6}, examples)
    outside = [country['id'] for country in committed_world['countries']
               if not band(stage_count(country, committed_world))[0] <= round(country['difficulty'] * 100)
               <= band(stage_count(country, committed_world))[1]]
    case('every committed difficulty lies in its stage-count band', not outside, outside)
    case('the committed world.json is in the writer form', render(json.loads(committed.decode('utf-8'))) == committed,
         'render(load(world.json)) differs from its bytes')

    with tempfile.TemporaryDirectory(prefix='lastlight-world-self-test-') as temporary:
        temporary = pathlib.Path(temporary)
        world = temporary / 'world.json'
        world.write_bytes(committed)
        code, text = run('--check', '--world', str(world))
        case('--check exits 0 on an unchanged copy of the catalogue', code == 0, f'exit {code}: {text.strip()}')

        loaded = json.loads(committed.decode('utf-8'))
        world.write_bytes((json.dumps(loaded, separators=(',', ':'), ensure_ascii=False) + '\n').encode('utf-8'))
        code, text = run('--check', '--world', str(world))
        case('--check exits 1 on a byte difference (the compact form)', code == 1, f'exit {code}: {text.strip()}')
        code, text = run('--world', str(world))
        case('--from-existing is the default and restores the bytes', code == 0 and world.read_bytes() == committed, f'exit {code}: {text.strip()}')

        perturbed = copy.deepcopy(loaded)
        perturbed['countries'][0]['difficulty'] = 9.99
        world.write_bytes(render(perturbed))
        code, text = run('--check', '--from-existing', '--world', str(world))
        case('--check exits 1 on a changed difficulty', code == 1, f'exit {code}: {text.strip()}')
        code, text = run('--from-existing', '--world', str(world))
        case('--from-existing recomputes difficulty only', code == 0 and world.read_bytes() == committed, f'exit {code}: {text.strip()}')

        extra = copy.deepcopy(perturbed)
        extra['zz_extra'] = {'kept': [1, 2]}
        extra['countries'][1]['zz_note'] = 'kept'
        world.write_bytes(render(extra))
        run('--world', str(world))
        carried = json.loads(world.read_bytes().decode('utf-8'))
        extra['countries'][0]['difficulty'] = loaded['countries'][0]['difficulty']
        case('--from-existing carries every other key, in order', carried == extra and list(carried) == list(extra)
             and list(carried['countries'][1]) == list(extra['countries'][1]), 'keys changed')

        geojson = synthetic_sources()
        small, large = temporary / 'small.geojson', temporary / 'large.geojson'
        small.write_bytes(geojson[0])
        large.write_bytes(geojson[1])
        base = dict(version=1, levels_per_population=10000000, min_levels=3, max_levels=24,
                    strategy_config={'max_roster': 12, 'recruit_cost': 100},
                    sources=[dict(url=BASE + name, sha256='0' * 64) for name in NAMES], countries=[],
                    stage_themes=[{'id': 'self-test-theme', 'props': ['crate']}])
        world.write_bytes(render(base))
        before = world.read_bytes()
        rebuild_argv = ['--rebuild', '--world', str(world), str(small), str(large)]
        code, text = run(*rebuild_argv)
        case('--rebuild exits 3 when the sources miss their sha256 pins', code == SOURCE_DRIFT and world.read_bytes() == before,
             f'exit {code}: {text.strip()}')
        code, text = run(*rebuild_argv, '--allow-source-drift')
        case('--rebuild exits 4 when stage counts change', code == STAGE_COUNT_CHANGE and world.read_bytes() == before,
             f'exit {code}: {text.strip()}')
        code, text = run(*rebuild_argv, '--allow-source-drift', '--allow-stage-count-change')
        rebuilt = json.loads(world.read_bytes().decode('utf-8'))
        pins = [hashlib.sha256(raw).hexdigest() for raw in geojson]
        case('--rebuild with both allowances writes and re-pins the sources', code == 0
             and [source['sha256'] for source in rebuilt['sources']] == pins, f'exit {code}: {text.strip()}')
        case('--rebuild carries every other top-level key, in order', list(rebuilt) == list(base)
             and all(rebuilt[key] == base[key] for key in base if key not in ('sources', 'countries')), list(rebuilt))
        ids = [country['id'] for country in rebuilt['countries']]
        stages = {country['id']: stage_count(country, rebuilt) for country in rebuilt['countries']}
        case('--rebuild builds the synthetic countries', ids == ['FRA', 'DEU', 'ISL'] and stages == {'FRA': 7, 'DEU': 9, 'ISL': 3}
             and rebuilt['countries'][2]['sea_neighbors'] != [], f'{ids} {stages}')
        values = {country['id']: country['difficulty'] for country in rebuilt['countries']}
        case('--rebuild sets each difficulty from its stage-count band',
             values == {country['id']: difficulty(country, rebuilt) for country in rebuilt['countries']}
             and all(band(stages[code])[0] <= round(value * 100) <= band(stages[code])[1] for code, value in values.items()),
             values)

        rebuilt['countries'][1]['level_override'] = 5
        world.write_bytes(render(rebuilt))
        authored = world.read_bytes()
        code, text = run(*rebuild_argv)
        case('--rebuild is reproducible and keeps each level_override', code == 0 and world.read_bytes() == authored,
             f'exit {code}: {text.strip()}')
        code, text = run('--check', *rebuild_argv)
        case('--check --rebuild exits 0 on a reproduced catalogue', code == 0, f'exit {code}: {text.strip()}')
        code, text = run('--check', '--world', str(world))
        case('--check exits 0 on a rebuilt catalogue', code == 0, f'exit {code}: {text.strip()}')

        large.write_bytes(synthetic_sources(fra_population=68000000)[1])
        code, text = run(*rebuild_argv)
        case('--rebuild exits 3 on changed source bytes', code == SOURCE_DRIFT and world.read_bytes() == authored,
             f'exit {code}: {text.strip()}')
        code, text = run(*rebuild_argv, '--allow-source-drift')
        drifted = json.loads(world.read_bytes().decode('utf-8'))
        case('--allow-source-drift re-pins when no stage count changes', code == 0
             and drifted['sources'][1]['sha256'] == hashlib.sha256(large.read_bytes()).hexdigest()
             and drifted['countries'][0]['population'] == 68000000, f'exit {code}: {text.strip()}')
        accepted = world.read_bytes()
        large.write_bytes(synthetic_sources(fra_population=71000000)[1])
        code, text = run(*rebuild_argv, '--allow-source-drift')
        case('--rebuild exits 4 when one stage count changes', code == STAGE_COUNT_CHANGE and 'FRA 7 -> 8' in text
             and world.read_bytes() == accepted, f'exit {code}: {text.strip()}')

        for argv, name in [(['--rebuild', '--world', str(world)], '--rebuild without sources'),
                           (['--rebuild', '--world', str(world), str(small)], '--rebuild with one GeoJSON file'),
                           (['--rebuild', '--download', '--world', str(world), str(small), str(large)], '--rebuild with paths and --download'),
                           (['--world', str(world), str(small), str(large)], 'GeoJSON paths without --rebuild'),
                           (['--allow-source-drift', '--world', str(world)], 'an --allow flag without --rebuild'),
                           (['--self-test', '--check'], '--self-test with another option')]:
            code, text = run(*argv)
            case(f'{name} is a usage error (exit 2)', code == 2 and world.read_bytes() == accepted, f'exit {code}: {text.strip()}')

    failed = results.count(False)
    if failed:
        print(f'FAIL: {failed} of {len(results)} self-test cases')
        return 1
    print(f'PASS: {len(results)} self-test cases')
    return 0


def synthetic_sources(fra_population=67000000):
    """Tiny 110m and 50m GeoJSON stand-ins: two bordering squares and an island."""
    def square(x, y):
        return [[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1], [x, y]]

    def feature(code, name, x, y, population, subregion):
        return dict(type='Feature', geometry=dict(type='Polygon', coordinates=[square(x, y)]),
                    properties=dict(ADM0_A3=code, ADMIN=name, NAME_LONG=name, POP_EST=population, POP_YEAR=2019,
                                    SUBREGION=subregion, TYPE='Sovereign country', LABEL_X=x + 0.5, LABEL_Y=y + 0.5))
    land = [feature('DEU', 'Germany', 10, 50, 83000000, 'Western Europe'),
            feature('FRA', 'France', 9, 50, fra_population, 'Western Europe')]
    island = feature('ISL', 'Iceland', -20, 64, 360000, 'Northern Europe')
    return [json.dumps(dict(type='FeatureCollection', features=features)).encode('utf-8')
            for features in (land, land + [island])]


if __name__ == '__main__':
    sys.exit(main())
