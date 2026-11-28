"""Procedural art for the Barracks musket and its shot."""
import math

import bpy
from mathutils import Vector

from asset_helpers import box, cone, ico, material


NAME = 'methane_musket'
SLUG = 'methane_slug'
FLASH = 'methane_muzzle_flash'
GRIP_BONE = 'musket_grip'
GRIP_REST = (-.34, -.38, 1.19)
SUPPORT = (.012, -.30, .095)
BREECH = (.032, -.097, .257)
MUZZLE = (0, -1.06, .165)


def chamfered_plate(name, center, size, mat, bevel=.022):
    plate = box(name, center, size, mat)
    edges = plate.modifiers.new('Faceted edges', 'BEVEL')
    edges.width = bevel
    edges.segments = 1
    bpy.ops.object.modifier_apply(modifier=edges.name)
    return plate


def stock_strut(name, start, end, width, depth, mat):
    span = Vector(end) - Vector(start)
    center = Vector(start) + span / 2
    beam = chamfered_plate(name, center, (width, span.length, depth), mat, .012)
    beam.rotation_euler = span.to_track_quat('Y', 'Z').to_euler()


def model():
    ceramic = material('Nivalis cold enamel', (.20, .29, .33), metal=.22)
    frame = material('Blue graphite frame', (.065, .105, .13), metal=.30)
    edge = material('Frosted rail alloy', (.31, .39, .41), metal=.45)
    wrap = material('Basalt grip wrap', (.037, .051, .059))
    amber = material('Guard amber indicator', (.75, .28, .045), emission=1.1)
    ochre = material('Barracks ochre enamel', (.35, .17, .041))

    # Both hand centers and the shot line are defined in grip space.
    chamfered_plate('Trigger grip', (0, 0, 0), (.074, .14, .15), wrap, .015)
    chamfered_plate('Receiver', (0, -.073, .153), (.174, .32, .175), frame)
    chamfered_plate('Breech lid', (0, -.097, .237), (.143, .172, .048), ceramic, .014)

    chamfered_plate('Stock spine', (0, .211, .141), (.101, .315, .085), ceramic)
    stock_strut('Stock brace', (0, .042, .041), (0, .339, -.055), .076, .048, frame)
    chamfered_plate('Butt pad', (0, .349, .042), (.125, .066, .254), wrap)
    chamfered_plate('Ribbed cheek rest', (0, .238, .185), (.102, .205, .034), wrap, .010)

    chamfered_plate('Forward hand wrap', SUPPORT, (.102, .157, .084), wrap, .013)
    chamfered_plate('Fore-end shroud', (0, -.359, .175), (.13, .33, .11), ceramic)
    chamfered_plate('Top rail', (0, -.371, .231), (.063, .295, .029), edge, .009)
    barrel = cone('Long barrel', (0, -.735, MUZZLE[2]), .041, .57, frame, top=.035)
    barrel.rotation_euler.x = math.pi / 2
    for y in (-.514, -.896):
        band = cone('Barrel band', (0, y, MUZZLE[2]), .058, .08, edge, top=.056)
        band.rotation_euler.x = math.pi / 2
    # A dark recessed face gives the silhouette a readable muzzle at gameplay distance.
    shroud = cone('Muzzle collar', (0, -1.001, MUZZLE[2]), .054, .10, ceramic, top=.068)
    shroud.rotation_euler.x = math.pi / 2
    face = cone('Muzzle recess', (0, -1.052, MUZZLE[2]), .043, .006, wrap, top=.043)
    face.rotation_euler.x = math.pi / 2
    chamfered_plate('Front sight', (0, -.966, .219), (.036, .037, .057), edge, .008)
    chamfered_plate('Rear sight', (0, .024, .253), (.091, .040, .044), frame, .008)

    for side in (-1, 1):
        chamfered_plate('Service panel', (side * .084, -.086, .156),
                         (.023, .210, .100), ceramic, .011)
        box('Status slit', (side * .098, -.080, .174), (.006, .116, .024), amber)
        box('Guard stripe', (side * .068, -.413, .177), (.013, .080, .045), ochre)
        for y in (-.173, .007):
            box('Panel latch', (side * .097, y, .137), (.009, .023, .026), frame)

    muzzle = bpy.data.objects.new('MusketMuzzle', None)
    bpy.context.collection.objects.link(muzzle)
    muzzle.location = MUZZLE


def slug():
    metal = material('Glacier alloy slug', (.36, .48, .51), metal=.42)
    trace = material('Cooling slug wake', (.13, .56, .60), emission=1.4)
    core = cone('Slug', (0, 0, 0), .036, .13, metal, top=.018)
    core.rotation_euler.x = math.pi / 2
    wake = cone('Tapered wake', (0, .207, 0), .003, .31, trace, top=.025)
    wake.rotation_euler.x = math.pi / 2


def effect_group(action, name):
    group = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(group)
    group.animation_data_create()
    group.animation_data.action = action
    group.animation_data.action_slot = action.slots.new(id_type='OBJECT', name=name)
    return group


def key_effect(group, poses):
    for frame, forward, rise, scale in poses:
        group.location = (0, -forward, rise)
        group.scale = (scale,) * 3
        for channel in ('location', 'scale'):
            group.keyframe_insert(channel, frame=frame)


def muzzle_flash():
    blue = material('Methane blue-teal flame', (.028, .43, .66), emission=4.8)
    white = material('Ignition core', (.54, .94, .95), emission=6)
    smoke = material('Cold exhaust', (.18, .235, .26))
    smoke.surface_render_method = 'BLENDED'
    smoke.node_tree.nodes.get('Principled BSDF').inputs['Alpha'].default_value = .36
    action = bpy.data.actions.new('MethaneMuzzleFlash')
    ignition = effect_group(action, 'Ignition')
    puff = effect_group(action, 'Smoke')

    jet = cone('Teal flame', (0, -.186, 0), .104, .37, blue, top=.007)
    jet.rotation_euler.x = math.pi / 2
    jet.parent = ignition
    ico('Hot core', (0, -.070, 0), (.064, .123, .064), white).parent = ignition
    for x, y, z in [(-.079, -.096, .053), (.059, -.125, -.047), (.036, -.068, .083)]:
        spark = ico('Flame facet', (x, y, z), (.029, .088, .021), blue)
        spark.rotation_euler.z = math.atan2(x, -y)
        spark.parent = ignition
    for center, size in [((0, -.089, .021), (.10, .11, .10)),
                         ((.072, -.174, .084), (.093, .112, .091)),
                         ((-.065, -.151, .062), (.086, .103, .093))]:
        ico('Faceted exhaust', center, size, smoke).parent = puff

    key_effect(ignition, [(0, 0, 0, .78), (1, .023, 0, 1), (2, .09, .01, 0), (10, .09, .01, 0)])
    key_effect(puff, [(0, 0, 0, .16), (2, .095, .015, .64), (5, .21, .072, 1.15),
                      (7, .32, .15, 1.31), (10, .39, .205, 0)])
    bpy.context.scene.frame_set(0)
