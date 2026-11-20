"""Check the checked-in GLBs without requiring Blender or third-party packages."""
import json
from pathlib import Path
import struct

root = Path(__file__).resolve().parents[1]
documents = {}
for name in ['plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch']:
    source = root / 'art' / f'{name}.blend'
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
