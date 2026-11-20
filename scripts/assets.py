"""Run with blender --background -noaudio --factory-startup --python scripts/assets.py -- [--export]."""
import bpy
import math
import random
import sys
import json
import struct
from pathlib import Path
from mathutils import Vector, Matrix, Quaternion

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from grounding import ground_outpost
random.seed(19)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.context.scene.unit_settings.system = 'METRIC'
    bpy.context.scene.render.fps = 24


def material(name, color, emission=0, metal=0):
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*color, 1)
    m.use_nodes = True
    p = m.node_tree.nodes.get('Principled BSDF')
    p.inputs['Base Color'].default_value = (*color, 1)
    p.inputs['Roughness'].default_value = .65
    p.inputs['Metallic'].default_value = metal
    p.inputs['Emission Color'].default_value = (*color, 1)
    p.inputs['Emission Strength'].default_value = emission
    return m


def finish(name, mat, scale):
    o = bpy.context.object
    o.name = name
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if mat:
        o.data.materials.append(mat)
    return o


def ico(name, pos, scale, mat, subdivisions=1):
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdivisions, radius=1, location=pos)
    return finish(name, mat, scale)


def box(name, pos, scale, mat):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    return finish(name, mat, scale)


def cone(name, pos, radius, depth, mat, top=0):
    bpy.ops.mesh.primitive_cone_add(vertices=8, radius1=radius, radius2=top, depth=depth, location=pos)
    return finish(name, mat, (1, 1, 1))


def save_export(name):
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'art' / f'{name}.blend'))
    export(name)


def export(name):
    bpy.ops.export_scene.gltf(filepath=str(ROOT / 'assets/models' / f'{name}.glb'),
        export_format='GLB', export_animations=True, export_animation_mode='ACTIONS',
        export_force_sampling=True, export_yup=True, export_lights=False, export_cameras=False)


def character():
    reset()
    skin = material('Obsidian blue skin', (.055, .13, .18), metal=.15)
    cloth = material('Woven basalt cloak', (.07, .065, .095))
    glow = material('Living phosphor', (.18, .85, .78), 3)
    eye = material('Pale eyes', (.7, .95, 1), 5)
    parts = []
    def part(o, bone):
        parts.append((o, bone))
    part(ico('Torso', (0, 0, .95), (.28, .19, .39), skin, 2), 'spine')
    part(cone('Mantle', (0, 0, .94), .37, .57, cloth, .23), 'spine')
    part(ico('Cranium', (0, 0, 1.55), (.38, .26, .4), skin, 2), 'head')
    # Alien faces Blender -Y, exported as Bevy +Z.
    for side in [-1, 1]:
        part(ico('Eye', (side*.155, -.239, 1.59), (.085, .045, .12), eye, 2), 'head')
        part(cone('Sensory horn', (side*.28, .015, 1.93), .085, .38, skin), 'head')
        part(ico('Horn light', (side*.28, .015, 2.12), (.035, .035, .055), glow), 'head')
        part(ico('Arm', (side*.39, 0, 1.0), (.105, .105, .36), skin, 2), f'arm{side}')
        part(ico('Hand', (side*.42, -.02, .65), (.1, .11, .12), glow), f'arm{side}')
        part(ico('Leg', (side*.15, 0, .38), (.115, .13, .33), skin, 2), f'leg{side}')
        part(ico('Foot', (side*.15, -.09, .10), (.14, .22, .10), cloth, 2), f'leg{side}')
    for z in [.92, 1.06, 1.2]:
        part(ico('Chest rune', (0, -.21, z), (.045, .025, .035), glow), 'spine')
    bpy.ops.object.armature_add(location=(0, 0, 0))
    rig = bpy.context.object
    rig.name = 'PlutonianRig'
    bpy.ops.object.mode_set(mode='EDIT')
    bones = rig.data.edit_bones
    bones.remove(bones[0])
    specs = [('root', (0,0,0), (0,0,.2), None),
        ('spine', (0,0,.7), (0,0,1.3), 'root'),
        ('head', (0,0,1.3), (0,0,1.95), 'spine')]
    for side in [-1, 1]:
        specs += [(f'arm{side}', (side*.34,0,1.3), (side*.42,0,.65), 'spine'),
                  (f'leg{side}', (side*.15,0,.7), (side*.15,0,.1), 'root')]
    for name, head, tail, parent in specs:
        b = bones.new(name)
        b.head, b.tail = head, tail
        if parent:
            b.parent = bones[parent]
    bpy.ops.object.mode_set(mode='OBJECT')
    for o, bone in parts:
        group = o.vertex_groups.new(name=bone)
        group.add(list(range(len(o.data.vertices))), 1, 'REPLACE')
        mod = o.modifiers.new('Rig', 'ARMATURE')
        mod.object = rig
        o.parent = rig
    for name, frames, swing, bob in [('Idle', 48, .035, .025), ('Walk', 24, .55, .04),
                                      ('Run', 16, .85, .08), ('Jump', 24, .2, .025)]:
        rig.animation_data_create()
        rig.animation_data.action = bpy.data.actions.new(name)
        for f in range(1, frames+2):
            phase = (f-1)/frames * math.tau
            for bone in rig.pose.bones:
                bone.rotation_mode = 'XYZ'
                bone.rotation_euler = (0,0,0)
                bone.location = (0,0,0)
                if bone.name.startswith('leg'):
                    sign = -1 if bone.name.endswith('-1') else 1
                    bone.rotation_euler.x = math.sin(phase)*swing*sign if name != 'Jump' else -.35
                if bone.name.startswith('arm'):
                    sign = -1 if bone.name.endswith('-1') else 1
                    bone.rotation_euler.x = -math.sin(phase)*swing*sign if name != 'Jump' else -.8
                if bone.name == 'spine':
                    bone.location.y = math.sin(phase*2)*bob
                bone.keyframe_insert('rotation_euler', frame=f, group=bone.name)
                bone.keyframe_insert('location', frame=f, group=bone.name)
        rig.animation_data.action.use_fake_user = True
    rig.animation_data.action = None
    bpy.context.scene.frame_set(1)
    save_export('plutonian')


def world():
    reset()
    ice = material('Blue graphite ice', (.075,.13,.19), metal=.25)
    stone = material('Basalt', (.035,.05,.075))
    wall = material('Ceramic shell', (.11,.13,.16), metal=.35)
    warm = material('Amber habitation', (.95,.34,.065), 5)
    teal = material('Trail phosphor', (.08,.65,.7), 4)
    path = material('Wind polished ice', (.13,.22,.28), metal=.4)
    collision = []
    def solid(o):
        collision.append(o)
        return o
    # A single gently inclined ice sheet, with a safe central route.
    floor = solid(box('Ice shelf', (0,52,-.95), (96,160,1.5), ice))
    floor.rotation_euler.x = .015
    # Low ramp provides a grounded traversal test along the trail.
    vertices = []
    faces = []
    for i in range(17):
        y = 39+i
        h = .015*y-.98 + .6*math.sin(math.pi*i/16)+.012
        vertices.extend([(-4.5,y,h), (4.5,y,h), (-4.5,y,h-.8), (4.5,y,h-.8)])
        if i:
            j = (i-1)*4
            faces.extend([(j,j+1,j+5,j+4), (j+2,j+6,j+7,j+3),
                          (j,j+4,j+6,j+2), (j+1,j+3,j+7,j+5)])
    faces.extend([(0,2,3,1), (64,65,67,66)])
    mesh = bpy.data.meshes.new('Trail rise')
    mesh.from_pydata(vertices, [], faces)
    ramp = bpy.data.objects.new('Trail rise', mesh)
    bpy.context.collection.objects.link(ramp)
    ramp.data.materials.append(path)
    solid(ramp)
    # An inhabited outpost, open central courtyard.
    for i, (x,y) in enumerate([(-10,1), (10,3), (-13,-8), (12,-10)]):
        solid(cone(f'Habitat {i}', (x,y,2.2), 4.1, 5, wall, 2.8))
        cone('Habitat cap', (x,y,5), 3.1, 1.5, stone, 1)
        box('Lit doorway', (x,y-3.65,1.5), (1.4,.18,2.3), warm)
        for s in [-1,1]:
            box('Window', (x+s*1.6,y-3.3,2.6), (.7,.2,.4), warm)
    solid(cone('Beacon foundation', (0,-5,.55), 2.5, 1.3, stone, 2))
    solid(cone('Beacon spindle', (0,-5,4), .65, 6, wall, .38))
    for h in [3,4.3,5.6]:
        bpy.ops.mesh.primitive_torus_add(major_radius=1.1, minor_radius=.12, major_segments=16, minor_segments=6, location=(0,-5,h))
        finish('Beacon ring', stone, (1,1,1))
    # Landmark ribs on the trail.
    for y in range(17,108,15):
        x = math.sin(y*.08)*4
        for side in [-1,1]:
            cone('Waystone', (x+side*4,y,1.15), .45, 2.8, stone, .25)
            ico('Waystone lamp', (x+side*4,y,2.6), (.22,.22,.3), teal)
    # Ice cliffs leave a broad route between them.
    for i in range(64):
        y = random.uniform(-20,122)
        side = random.choice([-1,1])
        x = side*random.uniform(17,39)
        o = ico('Ice escarpment', (x,y,1.2), (random.uniform(3,8), random.uniform(4,10), random.uniform(4,13)), ice, 1)
        solid(o)
    # Shallow cave, entering from the south; core sits beneath its ceiling.
    for x in [-7,7]:
        solid(box('Cavern wall', (x,117,3), (3,24,9), ice))
    solid(box('Cavern back', (0,129,3), (17,3,9), ice))
    solid(box('Cavern ceiling', (0,117,7.7), (17,25,2), ice))
    solid(cone('Core plinth', (0,122,.95), 1.2, 1.6, stone, .9))
    for i in range(20):
        x = random.choice([-1,1])*random.uniform(4.7,5.4)
        y = random.uniform(106,127)
        cone('Cave crystal', (x,y,1.5), .3, random.uniform(1,3), teal)
    # Stars are geometry, so the exported environment needs no external textures.
    stars = material('Distant stars', (.6,.75,1), 3)
    for i in range(140):
        x,y,z = random.uniform(-190,190),random.uniform(-90,220),random.uniform(45,120)
        ico('Star', (x,y,z), (.10,.10,.10), stars)
    # A small distant Charon, away from the route.
    ico('Charon', (-72,100,65), (13,13,13), material('Charon rock', (.23,.26,.32)), 3)
    ground_outpost()
    save_export('outpost')


def collision_from_world():
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'art/outpost.blend'))
    prefixes = ('Ice shelf', 'Trail rise', 'Habitat ', 'Beacon foundation', 'Beacon spindle',
                'Ice escarpment', 'Cavern ', 'Core plinth', 'Waystone')
    for o in list(bpy.data.objects):
        if not o.name.startswith(prefixes) or o.name.startswith('Waystone lamp'):
            bpy.data.objects.remove(o, do_unlink=True)
    save_export('collision')


def add_pillar_collision():
    """Add missing pillar bodies without replacing existing authored collision meshes."""
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/collision.blend'))
    existing = {obj.name for obj in bpy.context.scene.objects}
    with bpy.data.libraries.load(str(ROOT / 'art/outpost.blend'), link=False) as (source, target):
        target.objects = [name for name in source.objects
                          if name.startswith('Waystone') and not name.startswith('Waystone lamp')
                          and name not in existing]
    for obj in target.objects:
        bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.update()
    print(f'Added {len(target.objects)} pillar collision meshes')
    save_export('collision')


def core():
    reset()
    metal = material('Core casing', (.15,.2,.24), metal=.8)
    light = material('Core heart', (.2,.9,1), 7)
    ico('Core heart', (0,0,0), (.26,.26,.45), light, 2)
    for z in [-.35,.35]:
        cone('Core casing', (0,0,z), .34,.18,metal,.28)
    save_export('core')


def items():
    """Build only material pickups, preserving existing world and character source art."""
    for name in ['battery', 'scrap', 'methane_pouch']:
        reset()
        metal = material('Salvaged alloy', (.13,.2,.24), metal=.7)
        dark = material('Insulated shell', (.035,.06,.07))
        glow = material('Identification phosphor', (.12,.85,.62) if name == 'battery' else (.9,.45,.13), 2)
        if name == 'battery':
            box('Battery cell', (0,0,.22), (.27,.21,.4), metal)
            for z in [.06,.39]:
                box('Cell band', (0,0,z), (.3,.24,.06), dark)
            box('Charge strip', (0,-.113,.25), (.12,.02,.2), glow)
            cone('Terminal', (0,0,.46), .055,.08,metal,.055)
        elif name == 'scrap':
            for i in range(3):
                part = box('Scrap plate', (0,(i-1)*.09,.045+i*.055), (.44-i*.05,.14,.07), metal)
                part.rotation_euler.z = (i-1)*.45
            box('Salvage marking', (0,-.15,.09), (.13,.025,.05), glow)
        else:
            pouch = material('Methane membrane', (.14,.3,.38), metal=.15)
            ico('Methane pouch', (0,0,.22), (.25,.15,.22), pouch, 2)
            cone('Sealed neck', (0,0,.44), .065,.12,metal,.05)
            box('Seal', (0,0,.51), (.15,.1,.035), dark)
            box('Pouch marking', (0,-.15,.23), (.12,.02,.09), glow)
        bpy.context.view_layer.update()
        bottom = min((o.matrix_world @ Vector(c)).z for o in bpy.context.scene.objects for c in o.bound_box)
        for obj in bpy.context.scene.objects:
            obj.location.z -= bottom
        bpy.context.view_layer.update()
        save_export(name)


GEAR_NAMES = ['field_armor', 'reinforced_armor', 'field_helmet', 'reinforced_helmet']


def attachment_frames():
    """Read the exported rig's actual rest frames; do not guess Blender bone axis conversion."""
    data = (ROOT / 'assets/models/plutonian.glb').read_bytes()
    document = json.loads(data[20:20+struct.unpack_from('<I', data, 12)[0]])
    nodes = document['nodes']
    parents = {child: i for i, node in enumerate(nodes) for child in node.get('children', [])}
    def world(index):
        node = nodes[index]
        if 'matrix' in node:
            matrix = Matrix([[node['matrix'][row+4*col] for col in range(4)] for row in range(4)])
        else:
            x,y,z,w = node.get('rotation', [0,0,0,1])
            matrix = (Matrix.Translation(Vector(node.get('translation', [0,0,0]))) @
                      Quaternion((w,x,y,z)).to_matrix().to_4x4() @
                      Matrix.Diagonal((*node.get('scale', [1,1,1]),1)))
        return world(parents[index]) @ matrix if index in parents else matrix
    convert = Matrix.Rotation(-math.pi/2, 4, 'X')
    result = {}
    for bone in ['spine', 'head']:
        index = next(i for i,node in enumerate(nodes) if node.get('name') == bone)
        result[bone] = convert.inverted() @ world(index).inverted() @ convert
    return result


def export_equipment(name):
    """Export bone-local worn geometry and a base-origin pickup from one editable source."""
    export(name + '_worn')
    bpy.context.view_layer.update()
    objects = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    matrices = {obj: obj.matrix_world.copy() for obj in objects}
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    low = Vector(tuple(min(p[axis] for p in corners) for axis in range(3)))
    high = Vector(tuple(max(p[axis] for p in corners) for axis in range(3)))
    offset = Vector((-(low.x+high.x)/2, -(low.y+high.y)/2, -low.z))
    for obj in objects:
        obj.matrix_world = Matrix.Translation(offset) @ matrices[obj]
    bpy.context.view_layer.update()
    export(name)
    for obj in objects:
        obj.matrix_world = matrices[obj]
    bpy.context.view_layer.update()


def equipment():
    frames = attachment_frames()
    for name in GEAR_NAMES:
        reset()
        reinforced = name.startswith('reinforced')
        plate = material('Plutonian field plate', (.10,.19,.23), metal=.65)
        edge = material('Basalt edging', (.025,.045,.065), metal=.3)
        phosphor = material('Equipment identification', (.95,.43,.10) if reinforced else (.15,.8,.68), 2.4)
        if name.endswith('armor'):
            # Fitted cuirass over the mantle; no arm pieces to obstruct limb animation.
            box('Chest plate', (0,-.31,1.0), (.54,.13,.51), plate)
            box('Back plate', (0,.28,1.0), (.53,.12,.5), plate)
            for side in [-1,1]:
                box('Side plate', (side*.29,0,1.0), (.12,.55,.47), edge)
                ico('Shoulder plate', (side*.3,0,1.24), (.11,.25,.085), plate, 2)
            for z in [.88,1.0,1.12]:
                box('Chest marking', (0,-.383,z), (.11,.025,.045), phosphor)
            if reinforced:
                for side in [-1,1]:
                    ico('Reinforced shoulder', (side*.32,0,1.26), (.17,.29,.105), plate, 2)
                    box('Outer brace', (side*.3,-.22,1.0), (.09,.22,.54), plate)
                box('Sternum guard', (0,-.40,1.0), (.19,.055,.42), edge)
                box('Guard marking', (0,-.433,1.07), (.08,.015,.15), phosphor)
            bone = 'spine'
        else:
            # Open face and narrow crown: the large eyes and both sensory horns stay visible.
            for side in [-1,1]:
                box('Helmet side', (side*.39,0,1.65), (.08,.48,.4), plate)
                box('Side identification', (side*.437,0,1.68), (.018,.2,.06), phosphor)
            box('Helmet back', (0,.26,1.66), (.64,.1,.4), plate)
            box('Brow guard', (0,-.26,1.81), (.73,.11,.1), edge)
            box('Crown between horns', (0,0,1.9), (.37,.59,.075), plate)
            box('Brow identification', (0,-.323,1.81), (.22,.015,.035), phosphor)
            if reinforced:
                for side in [-1,1]:
                    box('Helmet reinforcement', (side*.438,.02,1.65), (.065,.42,.42), edge)
                box('Rear reinforcement', (0,.325,1.67), (.57,.055,.35), edge)
                box('Crown stripe', (0,0,1.947), (.09,.5,.025), phosphor)
            bone = 'head'
        bpy.context.view_layer.update()
        for obj in list(bpy.context.scene.objects):
            obj.matrix_world = frames[bone] @ obj.matrix_world
        bpy.context.scene['equipment_bone'] = bone
        bpy.context.scene['equipment_item'] = name
        bpy.context.view_layer.update()
        bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'art' / f'{name}.blend'))
        export_equipment(name)


if '--equipment' in sys.argv:
    equipment()
elif '--pillars' in sys.argv:
    add_pillar_collision()
elif '--items' in sys.argv:
    items()
elif '--ground' in sys.argv:
    # Repair existing sources without replacing authored meshes or materials.
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/outpost.blend'))
    changes = ground_outpost()
    save_export('outpost')
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/collision.blend'))
    for obj in bpy.context.scene.objects:
        if obj.name in changes:
            matrix = obj.matrix_world.copy()
            matrix.translation.z += changes[obj.name]
            obj.matrix_world = matrix
    bpy.context.view_layer.update()
    save_export('collision')
elif '--export' in sys.argv:
    for name in ['plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch', *GEAR_NAMES]:
        bpy.ops.wm.open_mainfile(filepath=str(ROOT/'art'/f'{name}.blend'))
        if name in GEAR_NAMES:
            export_equipment(name)
        else:
            export(name)
else:
    character()
    world()
    collision_from_world()
    core()
    items()
    equipment()
print('Exported Plutonian RPG assets')
