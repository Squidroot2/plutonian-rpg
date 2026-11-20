"""Check the checked-in GLBs without requiring Blender or third-party packages."""
import json
from pathlib import Path
import struct

root = Path(__file__).resolve().parents[1]
documents = {}
gear_names = ['field_armor', 'reinforced_armor', 'field_helmet', 'reinforced_helmet']
for name in ['plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch', *gear_names, *[name+'_worn' for name in gear_names]]:
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
    assert len(document.get('scenes', [])) == 1, f'{name}: expected one scene'
    assert document.get('meshes'), f'{name}: no meshes'
    for image in document.get('images', []):
        assert 'uri' not in image or image['uri'].startswith('data:'), f'{name}: external image'
    for buffer in document.get('buffers', []):
        assert 'uri' not in buffer, f'{name}: external buffer'
    if name == 'plutonian':
        names = {a['name'] for a in document.get('animations', [])}
        assert names == {'Idle', 'Walk', 'Run', 'Jump'}, names
        assert document.get('skins'), 'Character must be rigged'
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

limits = {'field_armor': [.86,.76,.8], 'reinforced_armor': [1.02,.8,.84],
          'field_helmet': [.9,.58,.72], 'reinforced_helmet': [1,.64,.8]}
for name in gear_names:
    low, high = mesh_bounds(documents[name])
    assert abs(low[1]) < .001, f'{name}: pickup must have a base origin'
    assert all(abs(low[i]+high[i]) < .001 for i in [0,2]), f'{name}: pickup must be centered horizontally'
    assert all(high[i]-low[i] <= limits[name][i]+.001 for i in range(3)), f'{name}: pickup exceeds drop clearance: {low}, {high}'
    assert len(documents[name]['meshes']) == len(documents[name+'_worn']['meshes']), f'{name}: worn and pickup geometry must be exported together'
    print(f'{name}: grounded pickup bounds and worn export OK')
