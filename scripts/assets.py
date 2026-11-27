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
import perimeter
import crossing
import village
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
    add_sword_animations(rig)
    add_plutonian_death(rig)
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


def add_plutonian_death(rig):
    rig.animation_data_create()
    new_action(rig, 'PlutonianDeath')
    for frame, height, roll, brace, bend in [
        (0, 0, 0, 0, 0),
        (4, -.05, 12, 30, 18),
        (10, -.23, 36, 55, 35),
        (17, .20, 84, 20, 12),
        (24, .25, 89, 12, 8),
    ]:
        root_rotation = Matrix.Rotation(math.radians(roll), 3, 'Y')
        for bone in rig.pose.bones:
            # The sword sockets inherit the forearms' collapse without their own keys.
            if bone.name in SWORD_GRIPS.values():
                continue
            offset = Vector((0, 0, height)) if bone.name == 'root' else Vector((0, 0, 0))
            rotation = Matrix.Identity(3)
            if bone.name == 'root':
                rotation = root_rotation
            elif bone.name == 'spine':
                rotation = Matrix.Rotation(math.radians(bend * .6), 3, 'X')
            elif bone.name == 'head':
                rotation = Matrix.Rotation(math.radians(-bend * .5), 3, 'Y')
            elif bone.name in ('arm-1', 'arm1'):
                side = -1 if bone.name == 'arm-1' else 1
                rotation = Matrix.Rotation(math.radians(-brace), 3, 'X')
                rotation = Matrix.Rotation(math.radians(-side * brace * .45), 3, 'Y') @ rotation
            elif bone.name in ('leg-1', 'leg1'):
                rotation = Matrix.Rotation(math.radians(bend if bone.name == 'leg-1' else -bend * .5), 3, 'X')
            key_pose(bone, offset, rotation, frame)
    rig.animation_data.action = None
    for bone in rig.pose.bones:
        bone.location = (0, 0, 0)
        bone.rotation_euler = (0, 0, 0)


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


def bend_arm(rig, side, target, shoulder_offset=None):
    arm = rig.pose.bones[f'arm{side}']
    forearm = rig.pose.bones[f'forearm{side}']
    shoulder = arm.bone.head_local
    rest_elbow = forearm.bone.head_local
    rest_hand = forearm.bone.tail_local
    upper = rest_elbow - shoulder
    lower = rest_hand - rest_elbow
    # Bring the supporting shoulder forward to let the left arm reach across.
    if shoulder_offset is None:
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
    return arm, forearm, lower_pose


def key_arm(arm, forearm, frame):
    for bone in [arm, forearm]:
        for channel in ['location', 'rotation_euler', 'scale']:
            bone.keyframe_insert(channel, frame=frame, group=bone.name)


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
        arm, forearm, _ = bend_arm(rig, side, target)
        key_arm(arm, forearm, frame)


def copy_locomotion_for_arms(rig, prefix, base):
    from bpy_extras.anim_utils import action_get_channelbag_for_slot

    name = prefix + base
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
    return source.frame_range


def add_spear_locomotion(rig, bases):
    rig.animation_data_create()
    rig.animation_data.action = None
    for base in bases:
        # Both arms hold the same shaft-relative pose as their shared spine moves.
        for frame in copy_locomotion_for_arms(rig, 'Spear', base):
            set_spear_grip(rig, frame)
    reset_character_pose(rig)


SWORD_GRIPS = {-1: 'sword_grip_right', 1: 'sword_grip_left'}
SWORD_GUARD = (.42, -.27, .78, 16, -20, 0)


def add_sword_joints(rig):
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    bones = rig.data.edit_bones
    for side, name in SWORD_GRIPS.items():
        forearm = bones[f'forearm{side}']
        grip = bones.get(name) or bones.new(name)
        grip.parent = forearm
        grip.head = forearm.tail
        grip.tail = grip.head + Vector((0, 0, .12))
        grip.roll = 0
    bpy.ops.object.mode_set(mode='OBJECT')
    bpy.context.view_layer.update()


def set_sword_pose(rig, side, pose, frame):
    width, depth, height, yaw, pitch, _ = pose
    target = Vector((side * width, depth, height))
    arm, forearm, lower_pose = bend_arm(rig, side, target, Vector((side * .015, -.025, 0)))
    key_arm(arm, forearm, frame)
    grip = rig.pose.bones[SWORD_GRIPS[side]]
    rest = grip.bone.matrix_local.to_3x3()
    lower_rotation = lower_pose.to_3x3().normalized()
    inherited = lower_rotation @ forearm.bone.matrix_local.to_3x3().inverted() @ rest
    blade = (Matrix.Rotation(math.radians(side * yaw), 3, 'Z')
             @ Matrix.Rotation(math.radians(pitch), 3, 'X'))
    grip.rotation_mode = 'XYZ'
    grip.rotation_euler = (inherited.inverted() @ blade @ rest).to_euler('XYZ')
    grip.scale = (1 / lower_pose.to_scale().x,) * 3
    for channel in ['rotation_euler', 'scale']:
        grip.keyframe_insert(channel, frame=frame, group=grip.name)


def add_sword_locomotion(rig, bases):
    rig.animation_data_create()
    rig.animation_data.action = None
    for base in bases:
        for frame in copy_locomotion_for_arms(rig, 'Sword', base):
            for side in SWORD_GRIPS:
                set_sword_pose(rig, side, SWORD_GUARD, frame)
    reset_character_pose(rig)


def add_sword_animations(rig):
    add_sword_joints(rig)
    add_sword_locomotion(rig, ['Idle', 'Walk', 'Run', 'Jump', *DIRECTIONAL_WALKS])
    poses = [
        (0, SWORD_GUARD),
        (2, (.54, .015, 1.10, 85, -30, 12)),
        (5, (.33, -.52, .87, 9, -5, -10)),
        (7, (.14, -.42, .78, -58, 18, -16)),
        (12, SWORD_GUARD),
    ]
    for side, name in [(-1, 'SwordMainSlash'), (1, 'SwordOffSlash')]:
        new_action(rig, name)
        for frame in range(13):
            (start, before), (end, after) = next(
                (a, b) for a, b in zip(poses, poses[1:]) if a[0] <= frame <= b[0])
            blend = (frame - start) / (end - start)
            blend = blend * blend * (3 - 2 * blend)
            pose = tuple(a + (b - a) * blend for a, b in zip(before, after))
            twist = Matrix.Rotation(math.radians(side * pose[-1]), 3, 'Z')
            key_pose(rig.pose.bones['spine'], Vector((0, 0, 0)), twist, frame)
            set_sword_pose(rig, side, pose, frame)
            set_sword_pose(rig, -side, SWORD_GUARD, frame)
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


TENT_SOLID = 'Starting tent solid '


def starting_tent():
    """Build the west-facing shelter on the ice east of the courtyard."""
    ceramic = bpy.data.materials['Ceramic shell']
    basalt = bpy.data.materials['Basalt']
    amber = bpy.data.materials['Amber habitation']
    lining = material('Shelter ceramic lining', (.19, .225, .24), metal=.2)
    frame = material('Shelter graphite frame', (.065, .092, .108), metal=.5)
    floor = material('Shelter rubber decking', (.12, .14, .15))
    stripe = material('Shelter aisle edging', (.32, .23, .105))
    mattress = material('Shelter mattress', (.17, .21, .23))
    blanket = material('Shelter rust wool', (.40, .135, .065))
    pillow = material('Shelter wool pillow', (.38, .37, .30))
    crate = material('Shelter enamel green', (.13, .23, .205), metal=.25)

    # Local X follows the aisle. The floor surface is Z = 0 in this frame.
    origin = Vector((10, 3, -.825))
    base_radius = 3.6
    shoulder_radius = 2.98
    shoulder_height = 3.28
    crown_radius = 1.52
    crown_height = 4.55
    wall_thickness = .16
    corner = math.sqrt(2) - 1

    def block(name, location, dimensions, mat):
        return box('Starting tent ' + name, origin + Vector(location), dimensions, mat)

    def mesh(name, vertices, faces, mat):
        data = bpy.data.meshes.new(name)
        data.from_pydata(vertices, [], faces)
        data.update()
        obj = bpy.data.objects.new(name, data)
        bpy.context.collection.objects.link(obj)
        obj.location = origin
        obj.data.materials.append(mat)
        return obj

    def shell_panel(name, outside, inside, outer, inner=None):
        count = len(outside)
        faces = [tuple(range(count)), tuple(reversed(range(count, count * 2)))]
        for index in range(count):
            following = (index + 1) % count
            faces.append((index, index + count, following + count, following))
        obj = mesh(TENT_SOLID + name, outside + inside, faces, outer)
        if inner is not None:
            obj.data.materials.append(inner)
            obj.data.polygons[1].material_index = 1

    def octagon(radius, z):
        bevel = radius * corner
        return [Vector((x, y, z)) for x, y in [
            (-radius, -bevel), (-bevel, -radius), (bevel, -radius), (radius, -bevel),
            (radius, bevel), (bevel, radius), (-bevel, radius), (-radius, bevel),
        ]]

    def radius_at(height):
        return base_radius + (shoulder_radius - base_radius) * height / shoulder_height

    def facet(edge, low, high, inset=0):
        bottom = octagon(radius_at(low) - inset, low)
        top = octagon(radius_at(high) - inset, high)
        following = (edge + 1) % len(bottom)
        return [bottom[edge], bottom[following], top[following], top[edge]]

    def beam(name, start, end, width, mat):
        direction = end - start
        obj = block(name, (start + end) / 2, (width, width, direction.length + .035), mat)
        obj.rotation_mode = 'QUATERNION'
        obj.rotation_quaternion = direction.to_track_quat('Z', 'Y')

    shell_panel('deck', octagon(base_radius, 0), octagon(base_radius, -.18), floor)
    mesh(TENT_SOLID + 'threshold ramp',
         [(-4.23, -.78, -.22), (-3.58, -.78, -.22), (-3.58, .78, -.22), (-4.23, .78, -.22),
          (-4.23, -.78, -.15), (-3.58, -.78, 0), (-3.58, .78, 0), (-4.23, .78, -.15)],
         [(3, 2, 1, 0), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5),
          (2, 3, 7, 6), (3, 0, 4, 7)], frame)
    for edge in range(7):
        name = 'rear wall' if edge == 3 else 'sidewall'
        shell_panel(name, facet(edge, 0, shoulder_height),
                    facet(edge, 0, shoulder_height, wall_thickness), ceramic, lining)

    # The western facet is split around a walk-through hatch with clipped corners.
    jamb = .86
    opening_top = 2.46
    opening_shoulder = 1.92
    opening_corner = .61

    def hatch_point(y, height, inset=0):
        return Vector((-radius_at(height) + inset, y, height))

    for side in (-1, 1):
        outline = [(jamb, 0), (base_radius*corner, 0),
                   (shoulder_radius*corner, shoulder_height), (opening_corner, shoulder_height),
                   (opening_corner, opening_top), (jamb, opening_shoulder)]
        if side > 0:
            outline.reverse()
        shell_panel('hatch jamb',
                    [hatch_point(side*y, z) for y, z in outline],
                    [hatch_point(side*y, z, wall_thickness) for y, z in outline], ceramic, lining)
        block('aisle stripe', (-.05, side*.65, .007), (6.94, .042, .012), stripe)

    lintel = [(opening_corner, opening_top), (-opening_corner, opening_top),
              (-opening_corner, shoulder_height), (opening_corner, shoulder_height)]
    shell_panel('hatch lintel',
                [hatch_point(y, z) for y, z in lintel],
                [hatch_point(y, z, wall_thickness) for y, z in lintel], ceramic, lining)
    surround = [(-.94, .02), (-.94, 1.94), (-.66, 2.53),
                (.66, 2.53), (.94, 1.94), (.94, .02)]
    for inset in (-.065, wall_thickness + .065):
        for (y0, z0), (y1, z1) in zip(surround, surround[1:]):
            start, end = hatch_point(y0, z0, inset), hatch_point(y1, z1, inset)
            beam('hatch frame', start, end, .17, basalt)
            if z0 > .02 and z1 > .02:
                lip = Vector((math.copysign(.091, inset), 0, 0))
                beam('hatch amber inlay', start.lerp(end, .12) + lip,
                     start.lerp(end, .88) + lip, .047, amber)

    # A basalt collar and faceted cap echo the other shelters around the beacon.
    roof_base = octagon(shoulder_radius + .23, shoulder_height - .08)
    roof_top = octagon(crown_radius, crown_height)
    roof_inside = octagon(shoulder_radius - wall_thickness, shoulder_height - .12)
    roof_top_inside = octagon(crown_radius, crown_height - .17)
    for edge in range(8):
        following = (edge + 1) % 8
        shell_panel('roof facet',
                    [roof_base[edge], roof_base[following], roof_top[following], roof_top[edge]],
                    [roof_inside[edge], roof_inside[following],
                     roof_top_inside[following], roof_top_inside[edge]], basalt, lining)
        beam('roof collar', roof_base[edge], roof_base[following], .18, frame)
    shell_panel('roof crown', roof_top, roof_top_inside, basalt, lining)

    inner_base = octagon(base_radius - wall_thickness - .035, .1)
    inner_shoulder = octagon(shoulder_radius - wall_thickness - .035, shoulder_height - .08)
    for edge in range(8):
        beam('interior rib', inner_base[edge], inner_shoulder[edge], .11, frame)
        beam('vault rib', inner_shoulder[edge], roof_top_inside[edge], .085, frame)

    # Amber viewports sit on the two angled facets beside the entrance.
    for edge, center in ((0, .32), (6, .68)):
        for inset, mat, width, low, high in [
            (-.025, basalt, .22, 1.92, 2.34), (-.043, amber, .16, 2.0, 2.26),
            (wall_thickness + .025, basalt, .22, 1.92, 2.34),
            (wall_thickness + .043, amber, .16, 2.0, 2.26),
        ]:
            a, b, c, d = facet(edge, low, high, inset)
            vertices = [a.lerp(b, center-width), a.lerp(b, center+width),
                        d.lerp(c, center+width), d.lerp(c, center-width)]
            face = (3, 2, 1, 0) if inset > 0 else (0, 1, 2, 3)
            mesh('Starting tent viewport', vertices, [face], mat)

    # The large furnishings have matching collision bodies. Small details sit on top.
    block('solid cot base', (-.55, 1.72, .19), (2.5, 1.19, .38), frame)
    block('solid mattress', (-.55, 1.72, .435), (2.38, 1.11, .18), mattress)
    block('folded blanket', (-.16, 1.72, .548), (1.36, 1.12, .05), blanket)
    block('blanket fold', (-.72, 1.72, .582), (.22, 1.13, .036), blanket)
    for offset in (-.025, .025):
        block('blanket seam', (.41+offset, 1.72, .577), (.012, 1.1, .012), pillow)
    ico('Starting tent pillow', origin + Vector((-1.43, 1.73, .57)), (.29, .45, .12), pillow, 2)
    for x in (-1.55, .45):
        block('cot frame trim', (x, 1.12, .28), (.09, .05, .23), stripe)

    lamp_x, lamp_y = -2.16, 1.75
    for x, y, width, depth, height in [(lamp_x, lamp_y, .75, .66, .58),
                                       (-1.85, -1.69, 1.2, .81, .67),
                                       (-.57, -2.02, .9, .73, .45)]:
        block('solid supply crate', (x, y, height/2), (width, depth, height), crate)
        block('crate lid', (x, y, height-.046), (width+.05, depth+.05, .09), frame)
        # Keep the latch embedded in the chest with its face proud of the lid overhang.
        latch_y = y - math.copysign(depth/2 + .015, y)
        for side in (-1, 1):
            block('crate latch', (x+side*width*.27, latch_y, height-.105),
                  (.09, .05, .12), stripe)
    cone('Starting tent heat lamp base', origin + Vector((lamp_x, lamp_y, .64)),
         .16, .12, basalt, .09)
    ico('Starting tent heat lamp crystal', origin + Vector((lamp_x, lamp_y, .85)),
        (.11, .11, .20), amber)
    for angle in (0, math.tau / 3, 2 * math.tau / 3):
        x, y = math.cos(angle), math.sin(angle)
        beam('heat lamp guard', Vector((lamp_x + x*.14, lamp_y + y*.14, .66)),
             Vector((lamp_x + x*.18, lamp_y + y*.18, .96)), .039, frame)


def star_field(material, count=1600):
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=1)
    template = bpy.context.object.data
    base_vertices = [vertex.co.copy() for vertex in template.vertices]
    base_faces = [tuple(polygon.vertices) for polygon in template.polygons]
    bpy.data.objects.remove(bpy.context.object, do_unlink=True)

    vertices = []
    faces = []
    for _ in range(count):
        position = Vector((
            random.uniform(-220, 220),
            random.uniform(-100, 250),
            random.uniform(45, 190),
        ))
        radius = random.triangular(.045, .14, .065)
        offset = len(vertices)
        vertices.extend(position + vertex * radius for vertex in base_vertices)
        faces.extend(tuple(offset + index for index in face) for face in base_faces)

    mesh = bpy.data.meshes.new('Distant star field')
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    stars = bpy.data.objects.new('Distant star field', mesh)
    bpy.context.collection.objects.link(stars)
    stars.data.materials.append(material)
    return stars


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
    # One combined mesh keeps the abundant star field inexpensive to draw.
    stars = material('Distant stars', (.82,.9,1), 4)
    star_field(stars)
    # A small distant Charon, away from the route.
    ico('Charon', (-72,100,65), (13,13,13), material('Charon rock', (.23,.26,.32)), 3)
    ground_outpost()
    starting_tent()
    village.build(material, box, ico, cone)
    perimeter.build(material, box)
    save_export('outpost')


def collision_from_world():
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'art/outpost.blend'))
    prefixes = ('Ice shelf', 'Trail rise', 'Beacon foundation', 'Beacon spindle',
                'Ice escarpment', 'Cavern ', 'Core plinth', 'Waystone', TENT_SOLID,
                perimeter.SOLID_PREFIX, village.SOLID)
    for o in list(bpy.data.objects):
        if not o.name.startswith(prefixes) or o.name.startswith('Waystone lamp'):
            bpy.data.objects.remove(o, do_unlink=True)
    save_export('collision')


def rebuild_village():
    """Replace just the occupied huts in the editable outpost and collider scenes."""
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/outpost.blend'))
    village.remove_homes()
    village.build(material, box, ico, cone)
    save_export('outpost')

    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/collision.blend'))
    village.remove_homes()
    with bpy.data.libraries.load(str(ROOT / 'art/outpost.blend')) as (source, target):
        target.objects = [name for name in source.objects if name.startswith(village.SOLID)]
    for obj in target.objects:
        bpy.context.scene.collection.objects.link(obj)
    save_export('collision')


def rebuild_perimeter():
    """Replace the perimeter in the authored scenes and preserve the rest of the map."""
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    outpost = str(ROOT / 'art/outpost.blend')
    bpy.ops.wm.open_mainfile(filepath=outpost)
    perimeter.build(material, box)
    save_export('outpost')

    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/collision.blend'))
    perimeter.clear()
    with bpy.data.libraries.load(outpost, link=False) as (source, target):
        target.objects = [name for name in source.objects if name.startswith(perimeter.SOLID_PREFIX)]
    for obj in target.objects:
        bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.update()
    save_export('collision')


def build_crossing():
    reset()
    crossing.flats(material, box, ico)
    save_export('listening_flats')
    for obj in list(bpy.context.scene.objects):
        if not obj.name.startswith(('Flats solid ', perimeter.SOLID_PREFIX)):
            bpy.data.objects.remove(obj, do_unlink=True)
    save_export('listening_flats_collision')

    reset()
    perimeter.gate_leaf(box, (
        material('Rim ceramic', (.096, .122, .153), metal=.25),
        material('Rim aged alloy', (.169, .237, .268), metal=.46),
        material('Rim graphite panels', (.092, .139, .170), metal=.32),
    ))
    save_export('nivalis_gate')

    reset()
    crossing.vellik(material, box, ico)
    save_export('vellik')

    build_wagon()


def build_wagon():
    reset()
    crossing.wagon(material, box, ico)
    save_export('crossing_wagon')


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


MEAT_NAMES = ['worm_meat', 'roach_meat']


def meat_pickups():
    reset()
    flesh = material('Worm flesh', (.32, .09, .20))
    skin = material('Worm skin', (.14, .07, .14))
    for y, width in [(-.14, .15), (0, .17), (.14, .15)]:
        ico('Worm cut', (0, y, .082), (width, .093, .082), flesh, 2)
    ico('Ribbed skin', (0, 0, .071), (.177, .20, .07), skin, 2)
    save_export('worm_meat')

    reset()
    flesh = material('Roach flesh', (.57, .29, .11))
    shell = material('Roach shell', (.23, .091, .032), metal=.15)
    ico('Roach cut', (0, 0, .073), (.19, .23, .073), flesh, 2)
    for y, width in [(-.10, .15), (0, .20), (.10, .15)]:
        ico('Shell plate', (0, y, .12), (width, .067, .049), shell, 2)
    save_export('roach_meat')


def large_boulder():
    reset()
    stone = material('Icebound rock', (.16, .21, .25), metal=.12)
    rock = ico('Large Boulder', (0, 0, .40), (.54, .47, .40), stone, 2)
    for vertex in rock.data.vertices:
        vertex.co *= random.uniform(.87, 1.0)
    rock.location.z = -min(vertex.co.z for vertex in rock.data.vertices)
    save_export('large_boulder')


SPEAR_NAMES = ['field_spear', 'trident']
SHORTSWORD_NAME = 'shortsword'
GEAR_NAMES = ['field_armor', 'reinforced_armor', 'field_helmet', 'reinforced_helmet',
              *SPEAR_NAMES, SHORTSWORD_NAME]


def attachment_frames(bones=('spine', 'head', 'spear_grip')):
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
    for bone in bones:
        index = next(i for i,node in enumerate(nodes) if node.get('name') == bone)
        result[bone] = convert.inverted() @ world(index).inverted() @ convert
    return result


def sword_attachment_rotation():
    name = SWORD_GRIPS[-1]
    return attachment_frames((name,))[name].to_3x3().to_4x4()


def export_equipment(name):
    """Export bone-local worn geometry and a base-origin pickup from one editable source."""
    export(name + '_worn')
    bpy.context.view_layer.update()
    objects = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    matrices = {obj: obj.matrix_world.copy() for obj in objects}
    # Convert the worn mesh to character space before grounding the pickup.
    if name in SPEAR_NAMES or name == SHORTSWORD_NAME:
        frame = (attachment_frames()['spear_grip'] if name in SPEAR_NAMES
                 else sword_attachment_rotation()).inverted()
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
        if name == SHORTSWORD_NAME:
            shortsword_model()
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


def shortsword_model():
    reset()
    steel = material('Frosted blade alloy', (.40, .57, .61), metal=.7)
    hilt = material('Basalt hilt', (.06, .10, .13), metal=.55)
    wrap = material('Insulated grip', (.04, .07, .08))
    inlay = material('Shortsword identification', (.16, .83, .70), emission=2.4)

    vertices = []
    for y, width, ridge in [(-.105, .077, .024), (-.48, .066, .020), (-.58, .043, .016)]:
        vertices.extend([(0, y, ridge), (-width, y, 0), (0, y, -ridge), (width, y, 0)])
    vertices.append((0, -.675, 0))
    faces = [(3, 2, 1, 0)]
    for ring in (0, 4):
        for index in range(4):
            following = (index + 1) % 4
            faces.append((ring + index, ring + following, ring + 4 + following, ring + 4 + index))
    faces.extend((8 + index, 8 + (index + 1) % 4, 12) for index in range(4))
    mesh = bpy.data.meshes.new('Double-edged blade')
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    blade = bpy.data.objects.new('Double-edged blade', mesh)
    bpy.context.collection.objects.link(blade)
    blade.data.materials.append(steel)

    box('Guard', (0, -.096, 0), (.23, .053, .065), hilt)
    box('Guard inlay', (0, -.096, .032), (.12, .020, .010), inlay)
    box('Hand wrap', (0, 0, 0), (.083, .16, .077), wrap)
    for y in [-.054, -.027, 0, .027, .054]:
        box('Grip rib', (0, y, 0), (.089, .012, .083), hilt)
    ico('Pommel', (0, .094, 0), (.055, .033, .046), steel, 2)

    frame = sword_attachment_rotation()
    bpy.context.view_layer.update()
    for obj in bpy.context.scene.objects:
        obj.matrix_world = frame @ obj.matrix_world
    bpy.context.view_layer.update()
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'art' / f'{SHORTSWORD_NAME}.blend'))
    export_equipment(SHORTSWORD_NAME)


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



def relay_roach():
    reset()
    scene = bpy.context.scene
    scene.render.fps = 60
    shell = material('Roach carapace', (.18, .12, .09), metal=.25)
    dark = material('Roach limbs', (.065, .045, .035))
    eye = material('Amber eyes', (1, .38, .08), 3)
    bpy.ops.object.armature_add()
    rig = bpy.context.object
    rig.name = 'RoachRig'
    bpy.ops.object.mode_set(mode='EDIT')
    bones = rig.data.edit_bones
    bones.remove(bones[0])
    root = bones.new('Body')
    root.head, root.tail = (0,0,0), (0,0,.2)
    for side in [-1, 1]:
        for index in range(3):
            bone = bones.new(f'Leg{side}_{index}')
            bone.head = (side*.35, (index-1)*.3, 0)
            bone.tail = (side*.75, (index-1)*.42, -.25)
            bone.parent = root
    bpy.ops.object.mode_set(mode='OBJECT')
    def skin(obj, name):
        group = obj.vertex_groups.new(name=name)
        group.add(list(range(len(obj.data.vertices))), 1, 'REPLACE')
        obj.modifiers.new('Skeleton', 'ARMATURE').object = rig
        obj.parent = rig
    skin(ico('Armored abdomen', (0,.15,.02), (.48,.68,.24), shell, 2), 'Body')
    skin(ico('Head', (0,-.50,-.02), (.29,.25,.18), dark, 2), 'Body')
    for side in [-1, 1]:
        skin(ico('Eye', (side*.20,-.68,.02), (.07,.05,.06), eye), 'Body')
        antenna = cone('Antenna', (side*.23,-.85,.20), .018,.65,dark)
        antenna.rotation_euler.x = -.8
        antenna.rotation_euler.y = side*.3
        skin(antenna, 'Body')
        for index in range(3):
            limb = ico('Leg', (side*.57,(index-1)*.35,-.16), (.30,.055,.055), dark)
            limb.rotation_euler.y = side*.45
            skin(limb, f'Leg{side}_{index}')
    for name, frames in [('RoachIdle', 120), ('RoachCrawl', 36), ('RoachBite', 72), ('RoachDeath', 42)]:
        rig.animation_data_create()
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        for frame in range(frames+1):
            phase = frame / frames * math.tau
            for bone in rig.pose.bones:
                bone.rotation_mode = 'XYZ'
                bone.rotation_euler = (0,0,0)
                bone.location = (0,0,0)
                if bone.name.startswith('Leg') and name in ('RoachIdle', 'RoachCrawl'):
                    side = -1 if '-1' in bone.name else 1
                    index = int(bone.name[-1])
                    bone.rotation_euler.z = math.sin(phase + index*math.pi + (side+1)*math.pi/2) * (.35 if name == 'RoachCrawl' else .025)
                if bone.name == 'Body' and name == 'RoachBite':
                    bone.rotation_euler.x = -.4 * max(0, 1-abs(frame-24)/24)
                if bone.name == 'Body' and name == 'RoachDeath':
                    bone.rotation_euler.y = math.pi * frame / frames
                bone.keyframe_insert('location', frame=frame, group=bone.name)
                bone.keyframe_insert('rotation_euler', frame=frame, group=bone.name)
    rig.animation_data.action = None
    scene.frame_set(0)
    for bone in rig.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    save_export('relay_roach')


def fungal_thread(name, points, width, mat):
    """Bake a tapered strand from a polyline so GLB export preserves its shape."""
    curve = bpy.data.curves.new(name, 'CURVE')
    curve.dimensions = '3D'
    curve.bevel_depth = width
    curve.bevel_resolution = 0
    spline = curve.splines.new('POLY')
    spline.points.add(len(points)-1)
    for index, (point, position) in enumerate(zip(spline.points, points)):
        point.co = (*position, 1)
        point.radius = 1-.82*index/(len(points)-1)
    strand = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(strand)
    curve.materials.append(mat)
    bpy.ops.object.select_all(action='DESELECT')
    strand.select_set(True)
    bpy.context.view_layer.objects.active = strand
    bpy.ops.object.convert(target='MESH')
    return bpy.context.object


def hushcap_crown(dark, slate, frost, gill):
    segments = 42
    rings = [(.072, 1.61), (.29, 1.57), (.56, 1.46), (.73, 1.31),
             (.75, 1.24), (.68, 1.19), (.22, 1.15)]
    vertices = []
    for radius, height in rings:
        for index in range(segments):
            a = math.tau*index/segments
            wave = math.sin(a*5+.8)*.04 + math.cos(a*9)*.023
            vertices.append((math.cos(a)*(radius+wave),
                             math.sin(a)*(radius+wave)*.91+.045,
                             height + wave*.72))
    faces = [tuple(range(segments))]
    for row in range(len(rings)-1):
        for index in range(segments):
            next_index = (index+1) % segments
            faces.append((row*segments+index, (row+1)*segments+index,
                          (row+1)*segments+next_index, row*segments+next_index))
    faces.append(tuple(reversed(range(len(vertices)-segments, len(vertices)))))
    mesh = bpy.data.meshes.new('Layered Hushcap crown')
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    cap = bpy.data.objects.new(mesh.name, mesh)
    bpy.context.collection.objects.link(cap)
    for mat in (dark, slate, frost, gill):
        mesh.materials.append(mat)
    for face in mesh.polygons[1:-1]:
        row, sector = divmod(face.index-1, segments)
        veins = math.sin(sector*.79+row*.65) + math.sin(sector*1.61)
        face.material_index = (3 if row == 5 else
                               2 if row == 4 or veins > 1.44 else
                               1 if row == 3 or veins > .10 else 0)
    return cap


def hushcap_actions(rig):
    crown = rig.pose.bones['Crown']
    crown.rotation_mode = 'XYZ'
    # Frame 51 at 60 fps is the point the glob leaves the mouth in the game.
    # Each key specifies drop, pitch, and compression around the base of the crown.
    actions = {
        'HushcapIdle': [(0, 0, 0, 1), (30, .014, .031, 1.015),
                        (90, -.018, -.023, .99), (120, 0, 0, 1)],
        'HushcapSpit': [(0, 0, 0, 1), (27, -.045, .14, .95), (49, -.11, .29, .85),
                        (51, .02, -.11, 1.06), (57, .025, -.20, 1.035),
                        (78, -.032, .10, .98), (126, 0, 0, 1)],
        'HushcapDeath': [(0, 0, 0, 1), (9, -.18, .27, .88),
                         (33, -.63, -.74, .72), (48, -.67, -.83, .72)],
    }
    rig.animation_data_create()
    for name, keys in actions.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        for frame, drop, pitch, compression in keys:
            crown.location = (0, drop, 0)
            crown.rotation_euler = (pitch, 0, 0)
            crown.scale = (compression, compression, compression)
            for channel in ('location', 'rotation_euler', 'scale'):
                crown.keyframe_insert(channel, frame=frame, group=crown.name)
    rig.animation_data.action = None
    crown.matrix_basis = Matrix.Identity(4)
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()


def hushcap():
    reset()
    bpy.context.scene.render.fps = 60
    charcoal = material('Charcoal rind', (.057, .070, .095))
    slate = material('Blue slate laminae', (.16, .24, .29))
    frost = material('Deposited frost', (.52, .68, .71))
    stalk = material('Pale fibrous stalk', (.29, .39, .38))
    gill = material('Luminous gill tissue', (.17, .53, .41), .55)
    throat = material('Dark vent', (.021, .044, .048))
    charge = material('Phosphor in vent', (.32, .83, .57), 1.25)

    bpy.ops.object.armature_add()
    rig = bpy.context.object
    rig.name = 'HushcapRig'
    bpy.ops.object.mode_set(mode='EDIT')
    root = rig.data.edit_bones[0]
    root.name = 'Roots'
    root.head, root.tail = (0, 0, 0), (0, 0, .44)
    crown = rig.data.edit_bones.new('Crown')
    crown.head, crown.tail = (0, 0, .88), (0, 0, 1.38)
    crown.parent = root
    bpy.ops.object.mode_set(mode='OBJECT')

    def skin(obj, bone):
        group = obj.vertex_groups.new(name=bone)
        group.add(list(range(len(obj.data.vertices))), 1, 'REPLACE')
        obj.modifiers.new('Hushcap skeleton', 'ARMATURE').object = rig
        obj.parent = rig

    skin(ico('Root knot', (0, .04, .17), (.44, .36, .26), stalk, 2), 'Roots')
    skin(ico('Pleated stalk', (0, .05, .53), (.30, .28, .53), stalk, 2), 'Roots')
    for index in range(12):
        a = index*math.tau/12
        s, c = math.sin(a), math.cos(a)
        radius = .48 + (index % 4)*.067
        skin(fungal_thread('Root filament', [(c*.12, s*.12, .34),
                            (c*.31, s*.31, .13), (c*.47, s*.42, .037),
                            (c*radius, s*radius, .022)], .052, frost), 'Roots')
    skin(ico('Collar', (0, .027, .95), (.38, .37, .30), slate, 2), 'Crown')
    skin(hushcap_crown(charcoal, slate, frost, gill), 'Crown')
    for index in range(34):
        a = math.tau*index/34
        c, s = math.cos(a), math.sin(a)
        skin(fungal_thread('Radial gill', [(c*.24, s*.21+.045, 1.085),
                            (c*.38, s*.35+.045, 1.072),
                            (c*.56, s*.51+.045, 1.12), (c*.70, s*.63+.045, 1.216)],
                           .030, gill if index % 3 else frost), 'Crown')
    skin(ico('Spit vent cavity', (0, -.358, 1.005), (.22, .10, .14), throat, 2), 'Crown')
    skin(ico('Glowing throat', (0, -.434, 1.01), (.095, .031, .063), charge, 2), 'Crown')
    for side in [-1, 1]:
        lip = ico('Split vent lip', (side*.154, -.399, .99), (.079, .119, .142), slate, 2)
        lip.rotation_euler.y = side*.23
        skin(lip, 'Crown')
    hushcap_actions(rig)
    save_export('hushcap')


def mycelium():
    reset()
    pale = material('Mycelium fibers', (.57, .69, .63))
    living = material('Mycelium phosphor', (.27, .58, .41), .75)
    ico('Root sample', (0, 0, .075), (.143, .11, .075), pale, 2)
    for index in range(21):
        a = index*math.tau/21
        radius = .115+.012*(index % 9)
        lift = (index % 5)*.025
        points = [(0, 0, .09), (math.cos(a)*.08, math.sin(a)*.08, .13+lift),
                  (math.cos(a+.5)*radius, math.sin(a+.5)*radius, .072+lift),
                  (math.cos(a+.8)*radius*1.19, math.sin(a+.8)*radius*1.19, .035+lift)]
        fungal_thread('Mycelium strand', points, .024, pale if index % 4 else living)
    save_export('mycelium')


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
    console = (1.77, 23.55)
    solid('Archive service console', (*console, .69), (1.26, .72, 1.38), rib)
    box('Service console dark panel', (console[0], console[1]-.366, .93),
        (1.06, .04, .46), etched)
    for x in (1.42, 1.62, 1.82):
        box('Service console indicator', (x, console[1]-.394, 1.02), (.096, .02, .042), amber)
    box('Archive conduit from east wall', (4.52, 26.45, .125), (1.8, .24, .24), rib)
    crack = box('Dislodged conduit end', (3.61, 26.33, .083), (.54, .26, .16), etched)
    crack.rotation_euler.z = -.44
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


SOUTH_BASIN_MODELS = ['south_basin', 'south_basin_collision']


def south_basin_mesh(name, sections, materials, cap_start=True, cap_end=True):
    """Join cross sections ordered around the perimeter in Bevy X/Y/Z coordinates."""
    count = len(sections[0])
    vertices = [(x, -z, y) for section in sections for x, y, z in section]
    faces = []
    for index in range(len(sections)-1):
        start = index * count
        for side in range(count):
            a, b = start + side, start + (side+1) % count
            faces.extend([(a, a+count, b+count), (a, b+count, b)])
    if cap_start:
        faces.append(tuple(range(count)))
    if cap_end:
        faces.append(tuple(range(len(vertices)-1, len(vertices)-count-1, -1)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    for mat in materials:
        mesh.materials.append(mat)
    return obj


def south_basin_exterior(ice, frost, rock, rng):
    sections = []
    # The inlet slips just under the existing shelf where its southern edge meets the basin.
    for z, y, left, right in [(27.3, -1.43, -20, 20), (30, -1.44, -21, 21),
                              (41, -1.75, -26, 27), (51, -2.35, -29, 31),
                              (62, -2.05, -27, 31), (74, -1.82, -24, 30),
                              (84.05, -1.86, -16, 28)]:
        sections.append([(left, y-.2, z), (left*.53, y, z), (0, y-.19, z),
                         (right*.53, y, z), (right, y-.18, z), (right, y-3, z),
                         (left, y-3, z)])
    shelf = south_basin_mesh('South Basin shelf', sections, [ice, frost])
    for face in shelf.data.polygons:
        if face.index % 14 < 8 and rng.random() < .34:
            face.material_index = 1

    # The crest spans every ray from the settlement to the cave mouth.
    ridge_profiles = [
        (58, -6, 27, 6.0), (62, -9, 31, 11.0), (68, -10, 31, 12.5),
        (73, -8, 30, 10.5), (76, -2, 27, 5.5),
    ]
    ridge = south_basin_mesh('South Basin blind ridge', [
        [(left, -3, z), (left+1.5, height*.7, z), (left+6, height, z),
         ((left+right)/2+3, height+.8, z), (right-3, height*.79, z),
         (right, -3, z)] for z, left, right, height in ridge_profiles
    ], [rock, ice, frost])
    for face in ridge.data.polygons:
        face.material_index = [0, 1, 0, 2, 1, 0][face.index % 6]

    solids = {shelf.name, ridge.name}
    for x, z, width, depth, height in [
        (-6, 43, 2.4, 3.1, 3.9), (-2, 48, 2.3, 4.7, 4.0), (2, 51.3, 2.0, 3.1, 2.7),
        (-23, 50, 4.1, 9.0, 4.9), (25, 45, 5.4, 8.2, 4.5),
        (-21, 66, 3.7, 7.0, 4.2), (4, 83, 8.0, 4.0, 5.9),
        (25, 83, 7.1, 4.4, 6.6),
    ]:
        rock_fin = ico('South Basin ice fin', (x, -1.3, -z), (width, depth, height), frost, 2)
        rock_fin.rotation_euler.y = rng.uniform(-.4, .4)
        solids.add(rock_fin.name)
    return solids


def south_basin_tunnel(rock, ice, frost, rng):
    # (depth, center, half width, floor height, vault height), all in meters.
    profiles = [
        perimeter.CAVE_MOUTH, perimeter.CAVE_APPROACH,
        (94, 11, 3.6, -3.75, 5.1), (100, 13, 7, -5.50, 7.1),
        (104, 14, 12, -5.50, 8.2), (113, 14, 12, -5.50, 8.7),
        (117, 13, 4.5, -5.50, 5.8), (121, 12, 12, -5.50, 9.5),
        (132, 12, 12, -5.50, 9.4), (138, 14, 4.6, -5.50, 6.0),
        (142, 15, 12, -5.50, 10.6), (151, 14, 14, -5.50, 13.4),
        (158, 14, 11, -5.50, 11.2), (161, 14, 7, -5.50, 6.7),
    ]
    tunnel = south_basin_mesh('Hushcap Caverns shell', [
        [(x+dx*w, floor+height*h, z) for dx, height in perimeter.TUNNEL_RING]
        for z, x, w, floor, h in profiles
    ], [rock, ice, frost], cap_start=False)
    for face in tunnel.data.polygons:
        face.material_index = rng.choices([0, 1, 2], [6, 4, 1])[0]

    solids = {tunnel.name}
    for label, x, z, width, depth, height in [
        ('Entrance cover', 12.6, 103.2, 1.65, 1.18, 1.6),
        ('First chamber buttress', 17.6, 109.0, 1.8, 2.4, 3.7),
        ('Split chamber column', 12, 126.1, 4.2, 5.5, 7.3),
        ('West passage cover', 4.3, 129.4, 1.15, 1.65, 2.0),
        ('Gallery screen', 16.2, 143.0, 2.7, 2.4, 4.6),
        ('Gallery low cover', 10.8, 149.4, 1.65, 1.9, 1.8),
        ('Gallery vault pillar', 22.2, 153.4, 3.05, 4.0, 8.0),
    ]:
        formation = ico(label, (x, -z, -5.7+height*.53), (width, depth, height*.67), rock, 2)
        formation.rotation_euler.y = rng.uniform(-.23, .23)
        solids.add(formation.name)
    return solids


def south_basin_growth(ice, chalk, glow, rng):
    for x, z in [(7, 102), (23, 105), (4.3, 110), (20, 114), (2, 122), (22, 129),
                 (8.5, 134), (5, 144), (24, 147), (4.5, 152), (20, 156), (12, 159)]:
        for _ in range(5):
            px, pz = x+rng.uniform(-.9, .9), -z+rng.uniform(-1.1, 1.1)
            height = rng.uniform(.15, .65)
            radius = rng.uniform(.12, .36)
            cone('Pale fungal stalk', (px, pz, -5.5+height/2), radius*.24, height, chalk)
            ico('Phosphorescent cup', (px, pz, -5.5+height),
                (radius, radius*.83, radius*.27), glow, 2)
            side = rng.choice((-1, 1))
            fungal_thread('Mycelium on the floor',
                          [(px, pz, -5.47), (px+side*.24, pz+.08, -5.47),
                           (px+side*.42, pz+.31, -5.47), (px+side*.69, pz+.41, -5.47)],
                          .028, chalk)
        for _ in range(3):
            px, pz = x+rng.uniform(-1.2, 1.2), -z+rng.uniform(-1.3, 1.3)
            shard = cone('Ice needles', (px, pz, -5.31), rng.uniform(.07, .17), .44, ice)
            shard.rotation_euler.y = rng.uniform(-.45, .45)


def south_basin_guide_mushroom(shelf, chalk, beacon_glow, beacon_crown):
    """Place one tall bioluminescent mushroom beside the ridge bend, visible from Nivalis."""
    import bmesh

    bx, by = -11.5, -72.5
    lean_x, lean_y = -0.18, -0.08
    bpy.context.view_layer.update()
    inv = shelf.matrix_world.inverted()
    down = inv.to_3x3() @ Vector((0, 0, -1))

    def floor_z(x, y):
        hit, loc, _, _ = shelf.ray_cast(inv @ Vector((x, y, 10.0)), down)
        if not hit:
            raise RuntimeError(f'Guide mushroom point ({x:.2f}, {y:.2f}) missed South Basin shelf')
        return (shelf.matrix_world @ loc).z

    def revolved_mesh(name, rings, segments, materials, material_for_face):
        vertices = []
        for cx, cy, z, radius, wave_amp in rings:
            for index in range(segments):
                angle = math.tau * index / segments
                wave = (math.sin(angle * 7 + .5) * .65 + math.cos(angle * 11) * .35) * wave_amp
                r = radius + wave
                vertices.append((cx + math.cos(angle) * r, cy + math.sin(angle) * r, z + wave * .45))
        faces = [tuple(range(segments))]
        for row in range(len(rings) - 1):
            for index in range(segments):
                nxt = (index + 1) % segments
                a = row * segments + index
                b = row * segments + nxt
                c = (row + 1) * segments + nxt
                d = (row + 1) * segments + index
                faces.append((a, d, c, b))
        faces.append(tuple(reversed(range(len(vertices) - segments, len(vertices)))))
        mesh = bpy.data.meshes.new(name)
        mesh.from_pydata(vertices, [], faces)
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(mesh)
        bm.free()
        mesh.update()
        for mat in materials:
            mesh.materials.append(mat)
        for face in mesh.polygons[1:-1]:
            row, sector = divmod(face.index - 1, segments)
            face.material_index = material_for_face(row, sector)
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.collection.objects.link(obj)
        return obj

    gz = floor_z(bx, by)
    stalk_rings = [
        (bx + lean_x * t, by + lean_y * t, gz + h, r, 0.0)
        for h, r, t in [
            (-.20, .64, 0.0), (.02, .56, 0.0), (.40, .40, .08), (1.15, .28, .34),
            (1.95, .25, .66), (2.60, .32, .90), (2.98, .48, 1.0),
        ]
    ]
    stalk = revolved_mesh(
        'South Basin solid Guide mushroom stalk',
        stalk_rings,
        14,
        [chalk, beacon_crown],
        lambda row, _: 1 if row >= len(stalk_rings) - 2 else 0,
    )

    cap_x, cap_y = bx + lean_x, by + lean_y
    cap_rings = [
        (cap_x, cap_y, gz + h, r, amp)
        for r, h, amp in [
            (.08, 3.66, 0.0), (.52, 3.56, .02), (1.04, 3.34, .05), (1.44, 3.04, .07),
            (1.62, 2.80, .08), (1.50, 2.66, .06), (.94, 2.80, .02), (.42, 2.70, 0.0),
        ]
    ]
    revolved_mesh(
        'Guide mushroom cap',
        cap_rings,
        24,
        [beacon_glow, beacon_crown],
        lambda row, sector: 0 if row >= 3 or sector % 3 == 0 else 1,
    )

    for index in range(14):
        angle = math.tau * index / 14
        c, s = math.cos(angle), math.sin(angle)
        fungal_thread(
            'Guide mushroom gill',
            [
                (cap_x + c * .40, cap_y + s * .40, gz + 2.70),
                (cap_x + c * .92, cap_y + s * .92, gz + 2.77),
                (cap_x + c * 1.46, cap_y + s * 1.46, gz + 2.68),
            ],
            .036,
            beacon_glow,
        )

    for index in range(9):
        angle = math.tau * index / 9 + .22
        c, s = math.cos(angle), math.sin(angle)
        reach = 1.35 + (index % 3) * .26
        pts = [
            (bx + c * .42, by + s * .42, gz + .28),
            (bx + c * .76, by + s * .76, floor_z(bx + c * .76, by + s * .76) + .06),
            (bx + c * (reach * .78), by + s * (reach * .78),
             floor_z(bx + c * (reach * .78), by + s * (reach * .78)) + .038),
            (bx + c * reach, by + s * reach,
             floor_z(bx + c * reach, by + s * reach) + .024),
        ]
        fungal_thread('Guide mushroom root', pts, .058, chalk)

    return {stalk.name}


def south_basin():
    reset()
    rng = random.Random(47)
    ice = material('Blue graphite ice', (.075, .13, .19), metal=.25)
    frost = material('Layered silvery frost', (.15, .24, .31), metal=.22)
    rock = material('Violet basalt', (.071, .068, .105), metal=.08)
    chalk = material('Pale mycelium', (.52, .62, .51), .12)
    glow = material('Fungal phosphor', (.24, .86, .61), 2.4)
    beacon_glow = material('Guide fungal phosphor', (.24, .92, .66), 7.5)
    beacon_crown = material('Guide fungal crown', (.18, .72, .56), 3.4)
    solids = south_basin_exterior(ice, frost, rock, rng)
    solids.update(south_basin_tunnel(rock, ice, frost, rng))
    south_basin_growth(frost, chalk, glow, rng)
    solids.update(
        south_basin_guide_mushroom(
            bpy.data.objects['South Basin shelf'], chalk, beacon_glow, beacon_crown
        )
    )
    visual_name, collision_name = SOUTH_BASIN_MODELS
    save_export(visual_name)
    for obj in list(bpy.context.scene.objects):
        if obj.name not in solids:
            bpy.data.objects.remove(obj, do_unlink=True)
    save_export(collision_name)


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


if '--south-basin' in sys.argv and '--export' not in sys.argv:
    south_basin()
elif '--relay' in sys.argv and '--export' not in sys.argv:
    old_relay()
elif '--hushcap' in sys.argv and '--export' not in sys.argv:
    hushcap()
    mycelium()
elif '--roach' in sys.argv:
    relay_roach()
elif '--worm' in sys.argv:
    ice_worm()
elif '--spear' in sys.argv:
    spear_assets()
elif '--shortsword' in sys.argv and '--export' not in sys.argv:
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/plutonian.blend'))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    add_sword_animations(rig)
    save_export('plutonian')
    shortsword_model()
elif '--locomotion' in sys.argv:
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/plutonian.blend'))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    add_directional_walks(rig)
    add_spear_locomotion(rig, DIRECTIONAL_WALKS)
    add_sword_locomotion(rig, DIRECTIONAL_WALKS)
    save_export('plutonian')
elif '--equipment' in sys.argv:
    equipment()
elif '--pillars' in sys.argv:
    add_pillar_collision()
elif '--boulder' in sys.argv:
    large_boulder()
elif '--meat' in sys.argv:
    meat_pickups()
elif '--items' in sys.argv:
    items()
elif '--build-wagon' in sys.argv:
    build_wagon()
elif '--build-crossing' in sys.argv:
    rebuild_perimeter()
    build_crossing()
elif '--build-perimeter' in sys.argv:
    rebuild_perimeter()
elif '--build-village' in sys.argv:
    rebuild_village()
elif '--build-outpost' in sys.argv:
    world()
    collision_from_world()
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
elif '--export' in sys.argv and '--plutonian-death' in sys.argv:
    bpy.context.preferences.filepaths.save_version = 0
    bpy.context.preferences.filepaths.file_preview_type = 'NONE'
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/plutonian.blend'))
    rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    add_plutonian_death(rig)
    save_export('plutonian')
elif '--export' in sys.argv:
    names = ['outpost', 'collision'] if '--outpost' in sys.argv else [
        'plutonian', 'outpost', 'collision', 'core', 'battery', 'scrap', 'methane_pouch',
        'large_boulder', 'practice_dummy', 'ice_worm', 'relay_roach', 'hushcap', 'mycelium', 'old_relay',
        'old_relay_collision', *SOUTH_BASIN_MODELS, *crossing.MODELS, *MEAT_NAMES, *GEAR_NAMES,
    ]
    if '--shortsword' in sys.argv:
        names = ['plutonian', SHORTSWORD_NAME]
    elif '--hushcap' in sys.argv:
        names = ['hushcap', 'mycelium']
    elif '--relay' in sys.argv:
        names = ['old_relay', 'old_relay_collision']
    elif '--south-basin' in sys.argv:
        names = SOUTH_BASIN_MODELS
    elif '--crossing' in sys.argv:
        names = crossing.MODELS
    elif '--wagon' in sys.argv:
        names = ['crossing_wagon']
    for name in names:
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
    meat_pickups()
    large_boulder()
    equipment()
    practice_dummy()
    ice_worm()
    relay_roach()
    hushcap()
    mycelium()
    old_relay()
    south_basin()
    build_crossing()
print('Exported Plutonian RPG assets')
