"""Check the checked-in GLBs without requiring Blender or third-party packages."""
import json
from math import dist
from pathlib import Path
import struct

root = Path(__file__).resolve().parents[1]
documents = {}
gear_names = ['field_armor', 'reinforced_armor', 'field_helmet', 'reinforced_helmet',
              'field_spear', 'trident', 'shortsword']
small_loot_names = ['worm_meat', 'roach_meat', 'mycelium']
directional_walks = {'WalkBack', 'StrafeLeft', 'StrafeRight', 'WalkForwardLeft',
                     'WalkForwardRight', 'WalkBackLeft', 'WalkBackRight'}
for name in ['plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch', 'large_boulder', 'practice_dummy', 'ice_worm', 'relay_roach', 'hushcap', 'old_relay', 'old_relay_collision', 'south_basin', 'south_basin_collision', 'listening_flats', 'listening_flats_collision', 'nivalis_gate', 'vellik', 'crossing_wagon', *small_loot_names, *gear_names, *[name+'_worn' for name in gear_names]]:
    source_name = name.removesuffix('_worn')
    source = root / 'art' / f'{source_name}.blend'
    path = root / 'assets' / 'models' / f'{name}.glb'
    assert source.is_file(), f'Missing Blender source: {source}'
    data = path.read_bytes()
    magic, version, length = struct.unpack_from('<III', data)
    assert magic == 0x46546C67 and version == 2 and length == len(data), path
    size, chunk_type = struct.unpack_from('<II', data, 12)
    assert chunk_type == 0x4E4F534A, path
    document = json.loads(data[20:20 + size])
    documents[name] = document
    if name == 'plutonian':
        character_buffer = data[28 + size:]
    assert len(document.get('scenes', [])) == 1, f'{name}: expected one scene'
    assert document.get('meshes'), f'{name}: no meshes'
    for image in document.get('images', []):
        assert 'uri' not in image or image['uri'].startswith('data:'), f'{name}: external image'
    for buffer in document.get('buffers', []):
        assert 'uri' not in buffer, f'{name}: external buffer'
    if name == 'plutonian':
        animations = {clip['name']: clip for clip in document.get('animations', [])}
        locomotion = {'Idle', 'Walk', 'Run', 'Jump'} | directional_walks
        sword_slashes = {'SwordMainSlash', 'SwordOffSlash'}
        carry = {prefix + clip for prefix in ['Spear', 'Sword'] for clip in locomotion}
        assert animations.keys() == locomotion | carry | sword_slashes | {'SpearThrust', 'NemiDeath'}, animations.keys()
        assert len(animations) == len(document['animations']), 'Duplicate character animation names'
        assert document.get('skins'), 'Character must be rigged'
        assert any(node.get('name') == 'spear_grip' for node in document['nodes']), 'Spear grip bone is missing'
        thrust = animations['SpearThrust']
        duration = max(document['accessors'][sampler['input']]['max'][0] for sampler in thrust['samplers'])
        assert abs(duration-16/24) < .001, 'SpearThrust duration must match the in-game recovery'
        for clip in sword_slashes:
            duration = max(document['accessors'][sampler['input']]['max'][0]
                           for sampler in animations[clip]['samplers'])
            assert abs(duration-12/24) < .001, f'{clip} duration must match the in-game recovery'
        death = animations['NemiDeath']
        duration = max(document['accessors'][sampler['input']]['max'][0] for sampler in death['samplers'])
        assert abs(duration-1) < .001, 'NemiDeath must be a one-second collapse'

    if name == 'relay_roach':
        assert document.get('skins'), 'Roach must be skinned'
        assert {clip['name'] for clip in document['animations']} == {'RoachIdle', 'RoachCrawl', 'RoachBite', 'RoachDeath'}

    durations = {
        'ice_worm': {'WormIdle': 2.0, 'WormCrawl': .9, 'WormBite': 1.4, 'WormDeath': .7},
        'hushcap': {'HushcapIdle': 2.0, 'HushcapSpit': 2.1, 'HushcapDeath': .8},
    }.get(name)
    if durations:
        assert document.get('skins'), f'{name} must be skinned'
        assert {clip['name'] for clip in document['animations']} == durations.keys()
        for clip in document['animations']:
            length = max(document['accessors'][sampler['input']]['max'][0] for sampler in clip['samplers'])
            assert abs(length - durations[clip['name']]) < .001, f"{clip['name']}: unexpected duration"

    print(f'{name}: {len(document["meshes"])} meshes, {len(data):,} bytes OK')

# Every visible pillar body must also have matching collision geometry.
def pillars(document):
    return {node['name']: node for node in document['nodes']
            if node.get('name', '').startswith('Waystone')
            and not node['name'].startswith('Waystone lamp') and 'mesh' in node}

visual_pillars = pillars(documents['outpost'])
collision_pillars = pillars(documents['collision'])
assert visual_pillars, 'Outpost must contain trail pillars'
for name, visual in visual_pillars.items():
    assert name in collision_pillars, f'{name}: missing pillar collision mesh'
    collision = collision_pillars[name]
    for field, identity in [('translation', [0,0,0]), ('rotation', [0,0,0,1]), ('scale', [1,1,1]),
                            ('matrix', [1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1])]:
        assert all(abs(a-b) < 0.0001 for a,b in zip(visual.get(field, identity), collision.get(field, identity))), f'{name}: collision {field} differs from visuals'
print(f'Pillar collision coverage and transforms OK: {len(visual_pillars)} bodies')

# Gear pickup exports must rest at their base and fit the per-item drop query.
from itertools import product

def multiply(a, b):
    return [[sum(a[row][k]*b[k][col] for k in range(4)) for col in range(4)] for row in range(4)]

def local_matrix(node):
    if 'matrix' in node:
        return [[node['matrix'][row+col*4] for col in range(4)] for row in range(4)]
    x,y,z,w = node.get('rotation', [0,0,0,1])
    rotation = [[1-2*y*y-2*z*z, 2*x*y-2*z*w, 2*x*z+2*y*w],
                [2*x*y+2*z*w, 1-2*x*x-2*z*z, 2*y*z-2*x*w],
                [2*x*z-2*y*w, 2*y*z+2*x*w, 1-2*x*x-2*y*y]]
    scale = node.get('scale', [1,1,1])
    position = node.get('translation', [0,0,0])
    return [[rotation[row][col]*scale[col] for col in range(3)]+[position[row]] for row in range(3)]+[[0,0,0,1]]

def mesh_bounds(document):
    points = []
    def visit(index, parent):
        node = document['nodes'][index]
        matrix = multiply(parent, local_matrix(node))
        if 'mesh' in node:
            for primitive in document['meshes'][node['mesh']]['primitives']:
                accessor = document['accessors'][primitive['attributes']['POSITION']]
                for corner in product(*zip(accessor['min'], accessor['max'])):
                    points.append([sum(matrix[row][k]*(*corner,1)[k] for k in range(4)) for row in range(3)])
        for child in node.get('children', []):
            visit(child, matrix)
    for index in document['scenes'][0]['nodes']:
        visit(index, [[int(row==col) for col in range(4)] for row in range(4)])
    return [min(p[axis] for p in points) for axis in range(3)], [max(p[axis] for p in points) for axis in range(3)]

def position_bounds(document, node):
    return [(document['accessors'][part['attributes']['POSITION']]['min'],
             document['accessors'][part['attributes']['POSITION']]['max'])
            for part in document['meshes'][node['mesh']]['primitives']]


def check_static_pair(asset, required, collision_asset=None, prefix=''):
    scene = documents[asset]
    collision = documents[collision_asset or asset + '_collision']
    visuals = {node['name']: node for node in scene['nodes'] if 'mesh' in node}
    solids = {node['name']: node for node in collision['nodes']
              if 'mesh' in node and node['name'].startswith(prefix)}
    if prefix:
        assert solids.keys() == {name for name in visuals if name.startswith(prefix)}, (
            f'{asset}: {prefix.strip()} visual and collision mesh names differ')
    assert required <= solids.keys(), f'{asset}: missing colliders {required - solids.keys()}'
    for name, collider in solids.items():
        assert name in visuals, f'{name}: no matching visual'
        visual = visuals[name]
        assert local_matrix(visual) == local_matrix(collider), f'{name}: collider misaligned'
        assert position_bounds(scene, visual) == position_bounds(collision, collider), f'{name}: collider bounds differ'
    print(f'{asset}: static collision matches visuals for {len(solids)} bodies')


perimeter_prefix = 'Perimeter solid '
check_static_pair('outpost', {perimeter_prefix + name for name in (
    'Shelf cliffs', 'Basin west cliffs', 'Cavern headwall', 'Gate lift housing', 'Gate header',
    'Gate east buttress', 'Gate west buttress', 'Gate east infill', 'Gate west infill',
    'Gate east jamb', 'Gate west jamb',
)}, 'collision', perimeter_prefix)

check_static_pair('listening_flats', {'Flats solid Floor', 'Flats solid Charon overlook',
                                       'Perimeter solid Listening Flats cliffs'})
for name in ('outpost', 'collision'):
    assert all(node.get('name') != perimeter_prefix + 'Gate leaf' for node in documents[name]['nodes'])
gate = documents['nivalis_gate']
leaf = next(node for node in gate['nodes'] if node['name'] == 'Nivalis gate leaf')
assert len(leaf['children']) == 7, 'The gate panels and bands must move with the leaf'
low, high = mesh_bounds(gate)
assert max(abs(a-b) for a,b in zip(high, (5.025, 4.35, .55))) < .001
assert max(abs(a-b) for a,b in zip(low, (-5.025, -4.35, -.74))) < .001
for model, prefix, count in [('vellik', 'Vellik Leg ', 6), ('crossing_wagon', 'Wagon Wheel ', 4)]:
    moving = [node for node in documents[model]['nodes'] if node.get('name', '').startswith(prefix)]
    assert len(moving) == count and all(node.get('children') for node in moving), model

check_static_pair('old_relay', {'Archive service console'} | {
    f'{room} {surface}'
    for room in ['Vestibule', 'Junction', 'Maintenance', 'Gate passage', 'Archive']
    for surface in ['floor', 'ceiling']
})
check_static_pair('south_basin', {
    'South Basin shelf', 'South Basin blind ridge', 'Hushcap Caverns shell',
    'Entrance cover', 'First chamber buttress', 'Split chamber column',
    'West passage cover', 'Gallery screen', 'Gallery low cover', 'Gallery vault pillar',
    'South Basin ice fin', *(f'South Basin ice fin.{index:03}' for index in range(1, 8)),
})


limits = {'field_armor': [.86,.76,.8], 'reinforced_armor': [1.02,.8,.84],
          'field_helmet': [.9,.58,.72], 'reinforced_helmet': [1,.64,.8],
          'field_spear': [.28,.23,2.3], 'trident': [.58,.23,2.3], 'shortsword': [.26,.2,.9]}
for name in gear_names:
    low, high = mesh_bounds(documents[name])
    assert abs(low[1]) < .001, f'{name}: pickup must have a base origin'
    assert all(abs(low[i]+high[i]) < .001 for i in [0,2]), f'{name}: pickup must be centered horizontally'
    assert all(high[i]-low[i] <= limits[name][i]+.001 for i in range(3)), f'{name}: pickup exceeds drop clearance: {low}, {high}'
    assert len(documents[name]['meshes']) == len(documents[name+'_worn']['meshes']), f'{name}: worn and pickup geometry must be exported together'
    print(f'{name}: grounded pickup bounds and worn export OK')


for name in small_loot_names:
    low, high = mesh_bounds(documents[name])
    assert abs(low[1]) < .001, f'{name}: pickup must have a base origin'
    tolerance = .08 if name == 'mycelium' else .001
    assert all(abs(low[i]+high[i]) < tolerance for i in [0,2]), f'{name}: pickup must be centered horizontally'
    assert all(high[i]-low[i] <= limit for i, limit in enumerate([.62, .6, .62])), f'{name}: pickup exceeds drop clearance'


# Reconstruct each sampled pose in spine space and check both hand centers
# against the wraps on the worn shaft. The spine's motion affects all three equally.
character = documents['plutonian']
nodes = {node['name']: index for index, node in enumerate(character['nodes'])}

def grip_wraps(document):
    return {node['name']: node.get('translation', [0,0,0]) for node in document['nodes']
            if node.get('name') in ['Rear hand wrap', 'Forward hand wrap']}


wraps = grip_wraps(documents['field_spear_worn'])
trident_wraps = grip_wraps(documents['trident_worn'])
assert wraps.keys() == trident_wraps.keys() and all(dist(wraps[name], trident_wraps[name]) < .001 for name in wraps), (
    'The trident must use the same two grip points as the field spear'
)
assert abs(mesh_bounds(documents['trident_worn'])[1][2] - mesh_bounds(documents['field_spear_worn'])[1][2]) < .001, (
    'The trident and field spear tips must have the same forward reach'
)


def read_samples(index):
    accessor = character['accessors'][index]
    view = character['bufferViews'][accessor['bufferView']]
    width = {'SCALAR': 1, 'VEC3': 3, 'VEC4': 4}[accessor['type']]
    offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
    stride = view.get('byteStride', width*4)
    return [struct.unpack_from('<' + 'f'*width, character_buffer, offset + i*stride)
            for i in range(accessor['count'])]


for animation in character['animations']:
    clip = animation['name']
    if clip.removeprefix('Spear').removeprefix('Sword') not in directional_walks:
        continue
    tracks = {(channel['target']['node'], channel['target']['path']):
              read_samples(animation['samplers'][channel['sampler']]['output'])
              for channel in animation['channels']}
    for (node, path), samples in tracks.items():
        if node == nodes['root']:
            assert len(set(samples)) == 1, f'{clip}: {path} moves the root'
    for name in ['leg-1', 'leg1']:
        rotations = tracks[nodes[name], 'rotation']
        assert len(set(rotations)) > 2 and dist(rotations[0], rotations[-1]) < .001, (
            f'{clip}: {name} must animate and loop'
        )


def transform_point(matrix, point):
    return [sum(row[i]*point[i] for i in range(3)) + row[3] for row in matrix[:3]]

def to_local(matrix, point):
    # Invert the rest transform's orthogonal columns to get the bone-local point.
    return [sum(matrix[row][col] * (point[row] - matrix[row][3]) for row in range(3))
            / sum(matrix[row][col]**2 for row in range(3)) for col in range(3)]


hands = [('arm-1', 'Rear hand wrap', [-.42, -.05, .02]),
         ('arm1', 'Forward hand wrap', [.42, -.05, .02])]
for animation in character['animations']:
    if not animation['name'].startswith('Spear'):
        continue
    tracks = {}
    for channel in animation['channels']:
        target = channel['target']
        sampler = animation['samplers'][channel['sampler']]
        tracks[target['node'], target['path']] = dict(zip(
            (round(time[0]*24) for time in read_samples(sampler['input'])),
            read_samples(sampler['output'])))
    grip_index = nodes['spear_grip']
    frames = tracks[grip_index, 'translation']
    thrust_grips = {}
    for frame in frames:
        def pose(index):
            node = character['nodes'][index].copy()
            for path in ['translation', 'rotation', 'scale']:
                samples = tracks.get((index, path))
                if samples:
                    if frame in samples:
                        node[path] = samples[frame]
                    else:
                        values = list(samples.values())
                        assert all(value == values[0] for value in values), (
                            f"{animation['name']}: missing varying {path} sample at {frame}"
                        )
                        node[path] = values[0]
            return local_matrix(node)

        grip = pose(grip_index)
        root_pose = pose(nodes['root'])
        model_grip = multiply(multiply(root_pose, pose(nodes['spine'])), grip)
        assert .70 <= model_grip[1][3] <= .95, f"{animation['name']} frame {frame}: grip moved away from hip height"
        assert grip[0][3] < -.2, f"{animation['name']} frame {frame}: shaft moved away from the right side"
        if animation['name'] != 'SpearThrust':
            assert grip[0][2] < -.2, f"{animation['name']} frame {frame}: shaft no longer angles outward"
        else:
            thrust_grips[frame] = transform_point(model_grip, (0, 0, 0))
            if frame == 6:
                assert abs(model_grip[0][2]) < .001 and abs(model_grip[1][2]) < .001, (
                    "SpearThrust: shaft must point straight and level at impact"
                )
                lead = nodes['leg1']
                rest_shoe = (.15, .10, .09)
                shoe = to_local(local_matrix(character['nodes'][lead]), rest_shoe)
                planted_shoe = transform_point(multiply(root_pose, pose(lead)), shoe)
                assert planted_shoe[2] > rest_shoe[2] + .22 and abs(planted_shoe[1] - rest_shoe[1]) < .06, (
                    "SpearThrust: lead foot must plant forward at impact"
                )
        for name, wrap, rest_center in hands:
            index = nodes[name]
            forearm = nodes['fore'+name]
            rest = multiply(local_matrix(character['nodes'][index]), local_matrix(character['nodes'][forearm]))
            posed_forearm = multiply(pose(index), pose(forearm))
            elbow = transform_point(posed_forearm, (0, 0, 0))
            assert elbow[2] > .38 or abs(elbow[0]) > .42, f"{animation['name']} frame {frame}: {name} elbow folds into the chest"
            hand_center = transform_point(posed_forearm, to_local(rest, rest_center))
            wrap_center = transform_point(grip, wraps[wrap])
            assert dist(hand_center, wrap_center) < .015, f"{animation['name']} frame {frame}: {name} lost the grip"
    if thrust_grips:
        assert thrust_grips[0][2] - thrust_grips[3][2] > .25, "SpearThrust: wind-up must draw the shaft back"
        assert thrust_grips[6][2] - thrust_grips[3][2] > .45, "SpearThrust: impact must drive the shaft forward"
        assert dist(thrust_grips[0], thrust_grips[16]) < .001, "SpearThrust: recovery must return to the carry pose"
print('Both hands align with the shaft and both elbows clear the body throughout the spear clips')

# The same worn mesh must center its hilt on either wrist. Check that the
# authored strikes move those wrists and return both to their guard positions.
wrap = next(node for node in documents['shortsword_worn']['nodes'] if node['name'] == 'Hand wrap')
assert dist(transform_point(local_matrix(wrap), (0, 0, 0)), (0, 0, 0)) < .001
parents = {child: index for index, node in enumerate(character['nodes']) for child in node.get('children', [])}
for hand, forearm in [('right', 'forearm-1'), ('left', 'forearm1')]:
    assert parents[nodes[f'sword_grip_{hand}']] == nodes[forearm], f'{hand} sword grip must follow the wrist'

def sword_wrist(animation, hand, frame):
    index = nodes[f'sword_grip_{hand}']
    tracks = {}
    for channel in animation['channels']:
        target = channel['target']
        sampler = animation['samplers'][channel['sampler']]
        times = [round(sample[0] * 24) for sample in read_samples(sampler['input'])]
        samples = read_samples(sampler['output'])
        tracks[target['node'], target['path']] = dict(zip(times, samples))

    def world(index):
        node = character['nodes'][index].copy()
        for path in ['translation', 'rotation', 'scale']:
            track = tracks.get((index, path))
            if track:
                node[path] = track.get(frame, next(iter(track.values())))
        matrix = local_matrix(node)
        return multiply(world(parents[index]), matrix) if index in parents else matrix

    return transform_point(world(index), (0, 0, 0))

for name, hand in [('SwordMainSlash', 'right'), ('SwordOffSlash', 'left')]:
    animation = next(clip for clip in character['animations'] if clip['name'] == name)
    guard, chamber, impact, recovery = [sword_wrist(animation, hand, frame) for frame in [0, 2, 5, 12]]
    assert impact[2] - chamber[2] > .4, f'{name}: striking hand must advance through contact'
    assert dist(guard, recovery) < .001, f'{name}: striking hand must return to guard'
print('Shortsword exports fit both animated wrist sockets')
