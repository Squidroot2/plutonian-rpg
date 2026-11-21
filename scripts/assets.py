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
    add_directional_walks(rig)
    add_spear_animations(rig)
    save_export('plutonian')


# Carry the shaft at the right hip. The alien's right is Blender -X.
SPEAR_GRIP = (-.40, -.42, .84)
SPEAR_HAND_SPACING = .23
SPEAR_CARRY_ANGLE = math.radians(-16)

# The rig faces -Y and its right side is -X in Blender.
DIRECTIONAL_WALKS = {
    'WalkBack': (0, 1),
    'StrafeLeft': (1, 0),
    'StrafeRight': (-1, 0),
    'WalkForwardLeft': (1, -1),
    'WalkForwardRight': (-1, -1),
    'WalkBackLeft': (1, 1),
    'WalkBackRight': (-1, 1),
}


def new_action(rig, name):
    previous = bpy.data.actions.get(name)
    if previous is not None:
        bpy.data.actions.remove(previous)
    action = bpy.data.actions.new(name)
    action.use_fake_user = True
    rig.animation_data.action = action


def key_pose(bone, offset, rotation, frame):
    rest = bone.bone.matrix_local.to_3x3()
    bone.location = rest.inverted() @ offset
    bone.rotation_mode = 'XYZ'
    bone.rotation_euler = (rest.inverted() @ rotation @ rest).to_euler('XYZ')
    bone.keyframe_insert('location', frame=frame, group=bone.name)
    bone.keyframe_insert('rotation_euler', frame=frame, group=bone.name)


def add_directional_walks(rig):
    rig.animation_data_create()
    frames = 20
    for name, (x, y) in DIRECTIONAL_WALKS.items():
        direction = Vector((x, y)).normalized()
        new_action(rig, name)
        for frame in range(frames + 1):
            cycle = (frame % frames) / frames
            key_pose(rig.pose.bones['spine'], Vector((0, 0, -.022 * math.sin(math.tau * cycle)**2)),
                     Matrix.Identity(3), frame)
            for side, shift in [(-1, 0), (1, .5)]:
                phase = (cycle + shift) % 1.0
                # The planted foot sweeps against motion, then lifts and recovers.
                swing = max(0.0, 2 * phase - 1)
                stride = 1 - 4 * phase if phase < .5 else 2 * swing**2 * (3 - 2 * swing) - 1
                lift = .10 * math.sin(math.pi * swing)**2
                lateral = direction.x * .13 * stride + side * abs(direction.x) * .11
                forward = direction.y * .24 * stride
                rotation = Vector((0, 0, -1)).rotation_difference(
                    Vector((lateral, forward, -.60))).to_matrix()
                # Account for the shoe's extent so the planted foot stays at floor height.
                shoe_center = rotation @ Vector((0, -.09, -.60))
                shoe_extent = Vector((.14 * rotation[2][0], .22 * rotation[2][1],
                                      .10 * rotation[2][2])).length
                leg = rig.pose.bones[f'leg{side}']
                height = shoe_extent - shoe_center.z - leg.bone.head_local.z + lift
                key_pose(leg, Vector((0, 0, height)), rotation, frame)
                arm_swing = -.24 * stride * direction.y
                key_pose(rig.pose.bones[f'arm{side}'], Vector((0, 0, 0)),
                         Matrix.Rotation(arm_swing, 3, 'X'), frame)
    rig.animation_data.action = None


def reset_character_pose(rig):
    rig.animation_data.action = None
    bpy.context.scene.frame_set(1)
    for bone in rig.pose.bones:
        if bone.name.startswith('arm'):
            bone.rotation_mode = 'XYZ'
        bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()


def add_spear_joints(rig):
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    bones = rig.data.edit_bones
    grip = bones.get('spear_grip') or bones.new('spear_grip')
    grip.head = SPEAR_GRIP
    grip.tail = Vector(SPEAR_GRIP) + Vector((0, 0, .15))
    grip.parent = bones['spine']
    for side in [-1, 1]:
        forearm = bones.get(f'forearm{side}') or bones.new(f'forearm{side}')
        forearm.head = (side*.39, 0, .92)
        forearm.tail = (side*.42, -.02, .65)
        forearm.parent = bones[f'arm{side}']
    bpy.ops.object.mode_set(mode='OBJECT')
    bpy.context.view_layer.update()

    # Blend the original arm through the elbow. Unarmed clips still move both
    # pieces together because the new joints inherit the original arm motion.
    for obj in rig.children:
        if obj.type != 'MESH':
            continue
        for side in [-1, 1]:
            upper = obj.vertex_groups.get(f'arm{side}')
            if upper is None:
                continue
            lower = obj.vertex_groups.get(f'forearm{side}') or obj.vertex_groups.new(name=f'forearm{side}')
            if obj.name.startswith('Hand'):
                lower.add(list(range(len(obj.data.vertices))), 1, 'REPLACE')
                upper.remove(list(range(len(obj.data.vertices))))
            elif obj.name.startswith('Arm'):
                for vertex in obj.data.vertices:
                    height = (obj.matrix_local @ vertex.co).z
                    bend = min(1.0, max(0.0, (.985-height)/.13))
                    upper.add([vertex.index], 1-bend, 'REPLACE')
                    lower.add([vertex.index], bend, 'REPLACE')


def bend_arm(rig, side, target):
    arm = rig.pose.bones[f'arm{side}']
    forearm = rig.pose.bones[f'forearm{side}']
    shoulder = arm.bone.head_local
    rest_elbow = forearm.bone.head_local
    rest_hand = forearm.bone.tail_local
    upper = rest_elbow - shoulder
    lower = rest_hand - rest_elbow
    # Bring the supporting shoulder forward to let the left arm reach across.
    shoulder_offset = Vector((-.065, -.15, -.045)) if side == 1 else Vector((-.02, -.04, 0))
    posed_shoulder = shoulder + shoulder_offset
    reach = target - posed_shoulder
    direction = reach.normalized()
    # Keep a bend even at full extension. Both segments stretch by the same
    # small amount so the wrist meets the grip without moving the shoulder.
    stretch = max(1.0, reach.length / (.94 * (upper.length + lower.length)))
    upper_length = upper.length * stretch
    lower_length = lower.length * stretch
    along = (upper_length**2 - lower_length**2 + reach.length_squared) / (2 * reach.length)
    outward = Vector((side, -.58, .22))
    pole = (outward - outward.dot(direction)*direction).normalized()
    elbow = posed_shoulder + along*direction + math.sqrt(max(0, upper_length**2 - along**2))*pole

    def posed_bone(bone, start, end, target_start, target_end):
        rotation = (end-start).rotation_difference(target_end-target_start)
        return (Matrix.Translation(target_start) @ rotation.to_matrix().to_4x4()
                @ Matrix.Scale(stretch, 4) @ Matrix.Translation(-start)
                @ bone.bone.matrix_local)

    upper_pose = posed_bone(arm, shoulder, rest_elbow, posed_shoulder, elbow)
    lower_pose = posed_bone(forearm, rest_elbow, rest_hand, elbow, target)
    arm.rotation_mode = forearm.rotation_mode = 'XYZ'
    arm.matrix_basis = arm.bone.matrix_local.inverted() @ upper_pose
    forearm.matrix_basis = (forearm.bone.matrix_local.inverted() @ arm.bone.matrix_local
                            @ upper_pose.inverted() @ lower_pose)
    return arm, forearm


def set_spear_grip(rig, frame, advance=0.0, lift=0.0, yaw=SPEAR_CARRY_ANGLE, lean=0.0):
    rear = Vector(SPEAR_GRIP) + Vector((0, -advance, lift))
    grip = rig.pose.bones['spear_grip']
    rest = grip.bone.matrix_local.to_3x3()
    # Keep the shaft level while the chest pitches forward under it.
    shaft_rotation = Matrix.Rotation(-lean, 3, 'X') @ Matrix.Rotation(yaw, 3, 'Z')
    grip.location = rest.inverted() @ (rear - Vector(SPEAR_GRIP))
    grip.rotation_mode = 'XYZ'
    grip.rotation_euler = (rest.inverted() @ shaft_rotation @ rest).to_euler('XYZ')
    for channel in ['location', 'rotation_euler']:
        grip.keyframe_insert(channel, frame=frame, group=grip.name)
    for side, spacing in [(-1, 0.0), (1, SPEAR_HAND_SPACING)]:
        target = rear + shaft_rotation @ Vector((0, -spacing, 0))
        for bone in bend_arm(rig, side, target):
            for channel in ['location', 'rotation_euler', 'scale']:
                bone.keyframe_insert(channel, frame=frame, group=bone.name)


def add_spear_locomotion(rig, bases):
    from bpy_extras.anim_utils import action_get_channelbag_for_slot

    rig.animation_data_create()
    rig.animation_data.action = None
    for base in bases:
        name = 'Spear' + base
        previous = bpy.data.actions.get(name)
        if previous is not None:
            bpy.data.actions.remove(previous)
        source = bpy.data.actions[base]
        action = source.copy()
        action.name = name
        action.use_fake_user = True
        rig.animation_data.action = action
        channelbag = action_get_channelbag_for_slot(action, rig.animation_data.action_slot)
        for curve in list(channelbag.fcurves):
            if curve.data_path.startswith(('pose.bones["arm1"]', 'pose.bones["arm-1"]')):
                channelbag.fcurves.remove(curve)
        # Both arms hold the same shaft-relative pose as their shared spine moves.
        for frame in source.frame_range:
            set_spear_grip(rig, frame)
    reset_character_pose(rig)


def add_spear_animations(rig):
    add_spear_joints(rig)
    add_spear_locomotion(rig, ['Idle', 'Walk', 'Run', 'Jump', *DIRECTIONAL_WALKS])
    new_action(rig, 'SpearThrust')

    def key_pitch(name, offset, pitch, frame):
        key_pose(rig.pose.bones[name], offset, Matrix.Rotation(pitch, 3, 'X'), frame)

    # Chamber the spear with a step, drive it forward at contact, then reset.
    # Columns after frame: grip advance/lift/yaw, chest drive/dip/lean,
    # lead leg pitch/step lift, rear leg pitch. Angles are in degrees.
    carry = (0, 0, math.degrees(SPEAR_CARRY_ANGLE), 0, 0, 0, 0, 0, 0)
    contact = (.09, .155, 0, .01, .04, 11, -28, 0, 16)
    poses = [
        (0, *carry),
        (2, -.15, .015, -17, -.045, .045, -4, -12, .075, 9),
        (3, -.22, .02, -10, -.065, .07, -7, -21, .055, 16),
        (6, *contact),
        (7, *contact),
        (11, -.04, .055, -12, -.015, .025, 3, -25, 0, 14),
        (13, -.035, .012, -18, -.01, .01, 0, -12, .065, 7),
        (16, *carry),
    ]
    for frame in range(17):
        start, end = next((start, end) for start, end in zip(poses, poses[1:])
                          if start[0] <= frame <= end[0])
        blend = (frame - start[0]) / (end[0] - start[0])
        blend = blend * blend * (3 - 2 * blend)
        advance, lift, yaw, drive, dip, lean, lead, step_lift, rear = (
            a + (b - a) * blend for a, b in zip(start[1:], end[1:])
        )
        lean = math.radians(lean)
        key_pitch('spine', Vector((0, -drive, -dip)), lean, frame)
        set_spear_grip(rig, frame, advance, lift, math.radians(yaw), lean)
        for name, angle, foot_lift in [('leg1', lead, step_lift), ('leg-1', rear, 0)]:
            pitch = math.radians(angle)
            # Offset each leg by the change in its shoe's lowest point.
            shoe_center = Matrix.Rotation(pitch, 3, 'X') @ Vector((0, -.09, -.60))
            shoe_extent = math.hypot(.22 * math.sin(pitch), .10 * math.cos(pitch))
            height = shoe_extent - shoe_center.z - .70 + foot_lift
            key_pitch(name, Vector((0, 0, height)), pitch, frame)

    reset_character_pose(rig)


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


SPEAR_NAMES = ['field_spear', 'trident']
GEAR_NAMES = ['field_armor', 'reinforced_armor', 'field_helmet', 'reinforced_helmet', *SPEAR_NAMES]


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
    for bone in ['spine', 'head', 'spear_grip']:
        index = next(i for i,node in enumerate(nodes) if node.get('name') == bone)
        result[bone] = convert.inverted() @ world(index).inverted() @ convert
    return result


def export_equipment(name):
    """Export bone-local worn geometry and a base-origin pickup from one editable source."""
    export(name + '_worn')
    bpy.context.view_layer.update()
    objects = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    matrices = {obj: obj.matrix_world.copy() for obj in objects}
    # Convert the worn mesh to character space before grounding the pickup.
    if name in SPEAR_NAMES:
        frame = attachment_frames()['spear_grip'].inverted()
        for obj in objects:
            obj.matrix_world = frame @ matrices[obj]
        bpy.context.view_layer.update()
    placement = {obj: obj.matrix_world.copy() for obj in objects}
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    low = Vector(tuple(min(p[axis] for p in corners) for axis in range(3)))
    high = Vector(tuple(max(p[axis] for p in corners) for axis in range(3)))
    offset = Vector((-(low.x+high.x)/2, -(low.y+high.y)/2, -low.z))
    for obj in objects:
        obj.matrix_world = Matrix.Translation(offset) @ placement[obj]
    bpy.context.view_layer.update()
    export(name)
    for obj in objects:
        obj.matrix_world = matrices[obj]
    bpy.context.view_layer.update()


def equipment():
    frames = attachment_frames()
    for name in GEAR_NAMES:
        if name in SPEAR_NAMES:
            spear_model(frames['spear_grip'], name)
            continue
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


def spear_model(frame, name):
    reset()
    shaft = material('Dark alloy shaft', (.13,.23,.26), metal=.65)
    grip = material('Insulated grip', (.035,.055,.065))
    blade = material('Pale ice alloy', (.32,.52,.57), metal=.75)
    tint = (.63,.28,.95) if name == 'trident' else (.12,.87,.7)
    glow = material('Spear identification', tint, emission=3)
    x, rear, z = SPEAR_GRIP
    box('Octagonal shaft core', (x,rear-.72,z), (.065,1.8,.061), shaft)
    box('Rear hand wrap', (x,rear,z), (.094,.20,.088), grip)
    box('Forward hand wrap', (x,rear-SPEAR_HAND_SPACING,z), (.094,.20,.088), grip)
    box('End cap', (x,rear+.18,z), (.106,.085,.095), blade)
    if name == 'trident':
        box('Fork crosspiece', (x,rear-1.55,z), (.47,.11,.09), shaft)
        box('Fork inlay', (x,rear-1.55,z+.048), (.38,.045,.012), glow)
        for side, label in [(-1, 'Right'), (1, 'Left')]:
            tine_x = x + side*.195
            box(f'{label} tine base', (tine_x,rear-1.61,z), (.086,.17,.071), blade)
            tine = cone(f'{label} tine', (tine_x,rear-1.765,z), .062, .24, blade)
            tine.rotation_euler.x = math.pi/2
            tine.scale.y = .64
    else:
        box('Glowing collar', (x,rear-1.57,z), (.115,.095,.096), glow)
    point = cone('Center tine' if name == 'trident' else 'Spear point',
                 (x,rear-1.755,z), .095, .38, blade)
    point.rotation_euler.x = math.pi/2
    point.scale.y = .64
    bpy.context.view_layer.update()
    for obj in bpy.context.scene.objects:
        obj.matrix_world = frame @ obj.matrix_world
    bpy.context.scene['equipment_bone'] = 'spear_grip'
    bpy.context.view_layer.update()
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'art' / f'{name}.blend'))
    export_equipment(name)


def practice_dummy():
    reset()
    stand = material('Training stand alloy', (.055,.12,.16), metal=.58)
    pad = material('Woven target pad', (.34,.14,.085))
    paint = material('Training target bands', (.95,.44,.11), emission=1.0)
    box('Pedestal', (0,0,.075), (.72,.55,.15), stand)
    box('Spring post', (0,0,.53), (.16,.16,.9), stand)
    box('Target pad', (0,0,1.05), (.68,.32,.75), pad)
    for side in [-1,1]:
        for size, radius, y in [(0.04,.26,.175),(0.035,.16,.196),(0.028,.068,.216)]:
            bpy.ops.mesh.primitive_torus_add(major_segments=16, minor_segments=6,
                major_radius=radius, minor_radius=size, location=(0,side*y,1.04),
                rotation=(math.pi/2,0,0))
            finish('Target ring', paint, (1,1,1))
    box('Top marker', (0,0,1.46), (.31,.33,.08), stand)
    save_export('practice_dummy')


def ice_worm():
    reset()
    scene = bpy.context.scene
    scene.render.fps = 60
    shell = material('Glacial chitin', (.09, .25, .30), metal=.16)
    ridge = material('Ice-crusted plates', (.25, .49, .52))
    belly = material('Deep blue tissue', (.045, .105, .15))
    mouth = material('Warm throat', (1.0, .25, .045), 4)
    dark = material('Mouth cavity', (.026, .012, .016))
    fang = material('Frost fangs', (.66, .9, .92))

    # The origin is the collision center, 0.56 m above the ground. Forward is -Y.
    segments = [
        ('Head', -.40, .05, (.39, .37, .41)),
        ('Collar', -.06, -.07, (.40, .36, .36)),
        ('Body', .27, -.15, (.36, .34, .32)),
        ('Hindbody', .56, -.23, (.28, .29, .25)),
        ('Tail', .81, -.34, (.18, .25, .16)),
    ]
    bpy.ops.object.armature_add()
    rig = bpy.context.object
    rig.name = 'IceWormRig'
    bpy.ops.object.mode_set(mode='EDIT')
    bones = rig.data.edit_bones
    bones.remove(bones[0])
    for name, y, z, _ in segments:
        bone = bones.new(name)
        bone.head, bone.tail = (0, y, z), (0, y, z + .25)
    bpy.ops.object.mode_set(mode='OBJECT')

    def skin(obj, bone):
        group = obj.vertex_groups.new(name=bone)
        group.add(list(range(len(obj.data.vertices))), 1, 'REPLACE')
        obj.modifiers.new('Ice worm skeleton', 'ARMATURE').object = rig
        obj.parent = rig

    for name, y, z, (width, length, height) in segments:
        skin(ico(name + ' shell', (0, y, z), (width, length, height), shell, 2), name)
        skin(ico(name + ' plating', (0, y+.06, z+height*.49),
                 (width*.85, length*.63, height*.64), ridge), name)
        if name != 'Head':
            skin(ico(name + ' underside', (0, y-.05, z-height*.45),
                     (width*.65, length*.76, height*.60), belly), name)

    skin(ico('Recessed throat', (0, -.743, .05), (.235, .045, .215), dark, 2), 'Head')
    skin(ico('Inner mouth glow', (0, -.781, .05), (.108, .024, .100), mouth, 2), 'Head')
    bpy.ops.mesh.primitive_torus_add(major_segments=16, minor_segments=6, major_radius=.22,
                                    minor_radius=.045, location=(0,-.783,.05),
                                    rotation=(math.pi/2,0,0))
    skin(finish('Luminous jaws', mouth, (1, 1, 1)), 'Head')
    for index in range(8):
        angle = index * math.tau / 8
        radial = Vector((math.cos(angle), 0, math.sin(angle)))
        tooth = cone(f'Tooth {index+1}', Vector((0,-.822,.05)) + radial*.196, .045, .14, fang)
        tooth.rotation_euler = (-radial + Vector((0,-.38,0))).to_track_quat('Z', 'Y').to_euler()
        skin(tooth, 'Head')

    for name, frames in [('WormIdle', 120), ('WormCrawl', 54), ('WormBite', 84), ('WormDeath', 42)]:
        rig.animation_data_create()
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        for frame in range(frames + 1):
            phase = frame / frames * math.tau
            rear = max(0, 1 - abs(frame-26)/26)
            snap = max(0, 1 - abs(frame-33)/7)
            collapse = frame / frames
            for index, bone in enumerate(rig.pose.bones):
                bone.rotation_mode = 'XYZ'
                bone.location = (0,0,0)
                bone.rotation_euler = (0,0,0)
                if name in ('WormIdle', 'WormCrawl'):
                    amplitude = .13 if name == 'WormCrawl' else .026
                    wave = math.sin(phase-index*.9)
                    bone.location.x = wave * amplitude
                    bone.location.y = abs(wave) * amplitude * .24
                    bone.rotation_euler.y = wave*amplitude*1.4
                elif name == 'WormBite' and index < 3:
                    weight = 1 - index*.32
                    bone.location.y = (.36*rear - .07*snap)*weight
                    bone.location.z = (.20*snap - .13*rear)*weight
                    bone.rotation_euler.x = (.21*snap - .42*rear)*weight
                elif name == 'WormDeath':
                    bone.location.y = -.18*collapse*(1-index*.19)
                    bone.rotation_euler.z = collapse*(.72+index*.08)
                bone.keyframe_insert('location', frame=frame, group=bone.name)
                bone.keyframe_insert('rotation_euler', frame=frame, group=bone.name)
    rig.animation_data.action = None
    scene.frame_set(0)
    for bone in rig.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    save_export('ice_worm')


def old_relay():
    reset()
    rock = material('Relay basalt', (.092, .148, .19))
    floor = material('Worn slate', (.13, .22, .26), metal=.2)
    rib = material('Old relay alloy', (.20, .31, .36), metal=.52)
    etched = material('Inlaid tracks', (.13, .25, .30), metal=.25)
    amber = material('Maintenance amber', (1.0, .41, .10), 3.2)
    cyan = material('Archive cyan', (.15, .76, .86), 3.3)
    ice = material('Blue cave crystal', (.13, .31, .45), .16, .26)

    # +Y is deeper into the relay. World placement subtracts this from Bevy Z.
    collision = set()

    def solid(name, pos, size, mat=rock):
        obj = box(name, pos, size, mat)
        collision.add(obj.name)
        return obj

    def shell(name, left, right, near, far):
        width, length = right-left, far-near
        x, y = (right+left)/2, (far+near)/2
        solid(name+' floor', (x, y, -.25), (width, length, .5), floor)
        solid(name+' ceiling', (x, y, 4.95), (width, length, .5))
        # Subtle panels along the seam keep the low-lit rooms legible.
        for lane in [-1, 1]:
            edge = x + lane * (width/2-.36)
            box(name+' floor trim', (edge, y, .008), (.12, length-.14, .016), etched)

    def length_wall(name, x, near, far):
        solid(name, (x, (near+far)/2, 2.35), (.28, far-near, 4.7))

    def end_wall(name, y, left, right):
        solid(name, ((left+right)/2, y, 2.35), (right-left, .28, 4.7))

    shell('Vestibule', -2.6, 2.6, -2, 6)
    length_wall('Vestibule west wall', -2.6, -2, 6)
    length_wall('Vestibule east wall', 2.6, -2, 6)
    end_wall('Vestibule entry wall', -2, -2.6, 2.6)

    shell('Junction', -5.25, 5.25, 6, 15)
    length_wall('Junction west wall', -5.25, 6, 15)
    length_wall('Junction east bounds', 5.25, 6, 9)
    length_wall('Junction east return', 5.25, 12.5, 15)
    for side in [-1, 1]:
        left, right = sorted((side*2.6, side*5.25))
        end_wall('Junction entry shoulder', 6, left, right)
        left, right = sorted((side*2, side*5.25))
        end_wall('Junction gate shoulder', 15, left, right)

    shell('Maintenance', 5.25, 11.25, 8, 14)
    length_wall('Maintenance back wall', 11.25, 8, 14)
    end_wall('Maintenance near wall', 8, 5.25, 11.25)
    end_wall('Maintenance far wall', 14, 5.25, 11.25)
    solid('Switch plinth', (10.45, 10.75, .48), (.64, .82, .96), rib)
    box('Switch housing inlay', (10.10, 10.75, .70), (.055, .43, .19), amber)
    for level in (.42, 1.55):
        box('Maintenance service conduit', (11.02, 10.75, level), (.16, 4.1, .14), rib)
    for y in (9.3, 10.0, 11.5, 12.2):
        box('Maintenance conduit clamp', (10.93, y, 1.55), (.09, .12, .27), etched)
    for x in (3.1, 4.4, 5.7, 7.0, 8.3, 9.6):
        box('Maintenance guide light', (x, 12.01, .018), (.54, .12, .035), amber)

    shell('Gate passage', -2, 2, 15, 20)
    length_wall('Gate passage west wall', -2, 15, 20)
    length_wall('Gate passage east wall', 2, 15, 20)
    for side in [-1, 1]:
        box('Gate guide rail', (side*1.92, 17.4, 2.35), (.16, .48, 4.7), rib)
        box('Gate guide light', (side*1.80, 17.0, 2.2), (.08, .15, 2.9), cyan)
    box('Gate header', (0, 17.4, 4.48), (4.0, .52, .42), rib)

    shell('Archive', -5.5, 5.5, 20, 30)
    length_wall('Archive west wall', -5.5, 20, 30)
    length_wall('Archive east wall', 5.5, 20, 30)
    end_wall('Archive far wall', 30, -5.5, 5.5)
    for side in [-1, 1]:
        left, right = sorted((side*2, side*5.5))
        end_wall('Archive entry shoulder', 20, left, right)
        for y in (22.0, 25.0, 28.4):
            solid('Archive data cabinet', (side*4.75, y, 1.12), (.92, 1.28, 2.24), rib)
            for z in (1.28, 1.52, 1.76):
                box('Archive status slit', (side*4.26, y, z), (.065, .62, .065), cyan)
    solid('Survey log pedestal', (0, 27.30, .53), (1.36, .95, 1.06), rib)
    box('Archive map backdrop', (0, 29.78, 2.65), (5.3, .21, 2.23), rib)
    landmarks = [(-1.95, 2.15), (-1.25, 2.48), (-.52, 2.78), (.31, 2.58), (1.05, 3.06), (1.85, 3.30)]
    for x, z in landmarks:
        box('Survey route landmark', (x, 29.63, z), (.18, .08, .18), cyan)
    for (x, z), (next_x, next_z) in zip(landmarks, landmarks[1:]):
        path = box('Survey route trace', ((x+next_x)/2, 29.65, (z+next_z)/2),
                   (math.hypot(next_x-x, next_z-z), .05, .035), cyan)
        path.rotation_euler.y = -math.atan2(next_z-z, next_x-x)

    for x, y, tint in [(-2.35, 3.8, cyan), (2.35, 3.8, cyan),
                       (-5.0, 9.0, cyan), (-5.0, 13.1, amber), (10.99, 10.7, amber),
                       (-5.23, 23.6, cyan), (5.23, 23.6, cyan), (0, 29.6, cyan)]:
        box('Relay lamp housing', (x, y, 3.52), (.35, .58, .9), rib)
        box('Relay lamp tube', (x, y-.16, 3.52), (.38, .27, .69), tint)
    for x, y, z, pitch in [(-4.68, 7.5, .44, .2), (-4.95, 8.15, .62, -.30),
                            (4.75, 13.9, .45, -.22), (5.0, 13.55, .61, .18)]:
        crystal = cone('Ice along wall', (x, y, z), .27, z*1.9, ice)
        crystal.rotation_euler.y = pitch

    save_export('old_relay')
    for obj in list(bpy.context.scene.objects):
        if obj.name not in collision:
            bpy.data.objects.remove(obj, do_unlink=True)
    save_export('old_relay_collision')


def spear_assets():
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/plutonian.blend'))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    add_spear_animations(rig)
    save_export('plutonian')
    frame = attachment_frames()['spear_grip']
    for name in SPEAR_NAMES:
        spear_model(frame, name)
    practice_dummy()


if '--relay' in sys.argv:
    old_relay()
elif '--worm' in sys.argv:
    ice_worm()
elif '--spear' in sys.argv:
    spear_assets()
elif '--locomotion' in sys.argv:
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/plutonian.blend'))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    add_directional_walks(rig)
    add_spear_locomotion(rig, DIRECTIONAL_WALKS)
    save_export('plutonian')
elif '--equipment' in sys.argv:
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
    for name in ['plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch', 'practice_dummy', 'ice_worm', 'old_relay', 'old_relay_collision', *GEAR_NAMES]:
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
    practice_dummy()
    ice_worm()
    old_relay()
print('Exported Plutonian RPG assets')
