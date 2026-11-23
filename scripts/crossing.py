"""Authored terrain and caravan models for the Listening Flats."""

import math
import random

import bpy
from mathutils import Vector

import perimeter

MODELS = ['listening_flats', 'listening_flats_collision', 'nivalis_gate', 'vellik', 'crossing_wagon']
RIM = [
    (-25, 26.8), (-25, 45), (-37.5, 54), (-37, 73), (-43, 98),
    (-62, 112), (-111, 112), (-126, 97), (-128, 57), (-114, 38),
    (-77, 36), (-57, 45), (-43, 54), (-35, 48), (-35, 26.8),
]
FLOOR = -1.46


def block(box, name, at, size, mat):
    x, y, z = at
    width, height, depth = size
    return box(name, (x, -z, y), (width, depth, height), mat)


def flats(material, box, ico):
    ice = material('Worn graphite ice', (.081, .146, .211), metal=.22)
    basalt = material('Crossing basalt', (.053, .068, .105))
    frost = material('Crossing frost', (.145, .224, .295))
    pale = material('Beacon glass', (.12, .63, .72), emission=3.1)
    bronze = material('Old wagon metal', (.19, .15, .12), metal=.40)
    rng = random.Random(583)
    ground = [(x, FLOOR, z) for x, z in RIM]
    face = tuple(reversed(range(len(ground))))
    perimeter.mesh('Flats solid Floor', ground, [face], [ice])
    # Keep a notch low enough to see Charon from the overlook across the shelf.
    def crest_limit(at):
        return 19 if -116 <= at.x <= -98 and at.y <= 40 else 31

    perimeter.cliff('Listening Flats cliffs', RIM, (basalt, ice, frost), rng,
                    depth_scale=lambda _: .45, crest_limit=crest_limit)

    # The approach is wide enough for the wagons and leaves the crossing open after arrival.
    road = [
        (-30, 28), (-30, 42), (-34, 51), (-47, 57), (-61, 63),
        (-75, 67), (-89, 73), (-103, 78), (-107.5, 89), (-99, 97),
        (-82, 96), (-76.5, 94), (-65, 88), (-62, 72), (-61, 63),
    ]
    track_ice = material('Compressed crossing ice', (.105, .182, .237), metal=.30)
    road_mesh(road, track_ice)
    road_mesh([(-107.5, 89), (-112, 91)], track_ice, name="Track to the overlook")
    for j, (x, z) in enumerate(road[:-1]):
        if j % 2:
            continue
        normal = road_normal(road, j)
        for side in (-1, 1):
            dx, dz = normal * side*3.4
            block(box, 'Flats solid Crossing waystone', (x+dx, FLOOR+.58, z+dz),
                  (.56, 1.2, .56), basalt)
            block(box, 'Crossing waystone glass', (x+dx, FLOOR+1.2, z+dz), (.28, .24, .28), pale)
    for x, z, sx, sz, height in [
        (-69, 50, 5, 4, 3.0), (-80, 57, 6, 4, 4.1), (-84, 81, 8, 5, 4.6),
        (-71, 85, 5, 4, 3.4), (-103, 58, 8, 8, 6.0), (-111, 68, 6, 4, 3.4),
        (-63, 98, 6, 4, 4.2), (-54, 74, 6, 4, 5.1), (-115, 100, 3, 3, 3.2),
        (-97, 103, 4, 4, 3.4), (-92, 47, 3, 3, 2.5), (-117, 58, 3, 3, 3.0),
    ]:
        shard = ico('Flats solid Ice fin', (x, -z, FLOOR+.1), (sx, sz, height),
                    rng.choice([ice, frost, basalt]), 1)
        shard.rotation_euler.y = rng.uniform(-.2, .2)

    cairn(box, ico, basalt, bronze, pale)
    overlook(basalt, frost, box)


def road_normal(road, index):
    a, b = road[max(0, index-1)], road[min(len(road)-1, index+1)]
    return Vector((b[1]-a[1], a[0]-b[0])).normalized()


def road_mesh(road, material, name='Polished wagon track'):
    vertices, faces = [], []
    for i, (x, z) in enumerate(road):
        offset = road_normal(road, i)*1.5
        vertices.extend(((x-offset.x, FLOOR+.013, z-offset.y), (x+offset.x, FLOOR+.013, z+offset.y)))
        if i:
            k = 2*i
            faces.append((k-2, k, k+1, k-1))
    perimeter.mesh(name, vertices, faces, [material])


def cairn(box, ico, basalt, bronze, pale):
    for i in range(5):
        radius = 2.05-i*.27
        ico('Flats solid Bell cairn', (-101, -86, FLOOR+.22+i*.40),
            (radius, radius*.85, .44), basalt, 1)
    for x in (-102.2, -99.8):
        block(box, 'Flats solid Bell frame post', (x, FLOOR+1.65, 85.3), (.26, 3.3, .26), bronze)
    block(box, 'Flats solid Bell frame bar', (-101, FLOOR+3.3, 85.3), (2.7, .21, .25), bronze)
    for i, x in enumerate((-101.78, -101.25, -100.70, -100.15)):
        block(box, 'Wagon bell tether', (x, FLOOR+3.03, 85.3), (.045, .55, .045), bronze)
        ico('Hanging wagon bell', (x, -85.3, FLOOR+2.73-(i%2)*.16), (.18, .17, .22), bronze, 1)
    block(box, 'Cairn lamp', (-101, FLOOR+2.10, 86.6), (.28, .27, .28), pale)


def overlook(basalt, frost, box):
    # A broad grade and raised viewing terrace on the west branch.
    sections = [(-112, 91, FLOOR-.05, 3), (-115, 83, 2.4, 4),
                (-117, 77, 6.3, 4), (-117, 72, 9.4, 4), (-117, 67, 9.45, 4)]
    vertices, faces = [], []
    for x, z, y, half in sections:
        vertices.extend(((x-half, y, z), (x+half, y, z),
                         (x-half, FLOOR-.4, z), (x+half, FLOOR-.4, z)))
    for i in range(0, len(vertices)-4, 4):
        faces.extend(((i+1,i+5,i+4,i), (i+4,i+6,i+2,i), (i+3,i+7,i+5,i+1)))
    n = len(vertices)
    faces.append((n-4,n-2,n-1,n-3))
    perimeter.mesh('Flats solid Charon overlook', vertices, faces, [frost])
    for x in (-120.2, -113.8):
        block(box, 'Flats solid Overlook pillars', (x, 10.43, 67.5), (.64, 2.0, .7), basalt)


def local_parent(name, at, parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = parent
    obj.location = at
    return obj


def vellik(material, box, ico):
    slate = material('Vellik slate shell', (.115, .199, .276), metal=.22)
    frost = material('Vellik raised shell', (.199, .285, .316), metal=.17)
    hide = material('Vellik dark hide', (.072, .085, .117))
    horn = material('Vellik etched crown', (.218, .285, .284), metal=.11)
    amber = material('Vellik amber slit', (.83, .40, .12), emission=1.75)
    harness = material('Vellik ochre harness', (.31, .17, .105))

    ico('Barrel torso', (0, .22, 1.28), (.73, 1.03, .51), hide, 2)
    for y, z, sx, sy, sz, mat in [
        (-.38, 1.56, .74, .57, .59, slate),
        (.23, 1.67, .83, .65, .58, frost),
        (.88, 1.53, .66, .56, .49, slate),
    ]:
        plate = ico('Overlapping shell plate', (0, y, z), (sx, sy, sz), mat, 1)
        plate.rotation_euler.y = .12 if y > 0 else -.10
    ico('Lowered shield head', (0, -1.07, 1.33), (.51, .54, .34), slate, 1)
    box('Amber sensory slit', (0, -1.53, 1.40), (.63, .085, .075), amber)
    crown = local_parent('Vellik Crown', (0, -1.00, 1.48))
    for side in (-1, 1):
        tine = ico('Swept crown tine', (side*.36, -.09, .33), (.19, .23, .54), horn, 1)
        tine.parent = crown
        tine.rotation_euler.y = side*.50
        prong = ico('Crown split tip', (side*.22, -.22, .57), (.11, .14, .26), frost, 1)
        prong.parent = crown
        prong.rotation_euler.y = side*.30
    for i, y in enumerate((-.62, .11, .83)):
        for side, label in [(-1, 'L'), (1, 'R')]:
            leg = local_parent(f'Vellik Leg {label}{i}', (side*.51, y, 1.29))
            joint = ico('Vellik angled thigh', (side*.21, .04, -.27), (.20, .19, .46), slate, 1)
            joint.rotation_euler.y = -side*.44
            joint.parent = leg
            lower = ico('Vellik slender shin', (side*.33, .07, -.83), (.13, .149, .46), hide, 1)
            lower.parent = leg
            pad = ico('Vellik crescent foot', (side*.33, -.015, -1.20), (.25, .37, .11), frost, 1)
            pad.parent = leg
    box('Harness saddle', (0, .78, 1.95), (1.16, .32, .23), harness)
    for x in (-.60, .60):
        strap = box('Harness draught strap', (x, 1.06, 1.13), (.13, 1.14, .14), harness)
        strap.rotation_euler.x = .23


def wagon(material, box, ico):
    wood = material('Grey crossing timber', (.20, .16, .13))
    metal = material('Ice-cleated iron', (.115, .147, .172), metal=.35)
    canvas = material('Weathered ochre canvas', (.33, .194, .114))
    pale = material('Canvas seam', (.46, .294, .168))
    amber = material('Wagon lantern', (1.0, .49, .15), emission=3.4)
    box('Wagon deck', (0, 0, 1.10), (2.55, 3.85, .25), wood)
    for x in (-1.15, 1.15):
        box('Wagon side board', (x, 0, 1.56), (.2, 3.75, .73), wood)
    for y in (-1.80, 1.80):
        box('Wagon end board', (0, y, 1.48), (2.22, .20, .56), wood)

    canopy = []
    for y in (-1.68, -.55, .55, 1.68):
        canopy.append([(-1.23, 1.89, -y), (-1.21, 2.56, -y), (-.69, 3.07, -y),
                       (.69, 3.07, -y), (1.21, 2.56, -y), (1.23, 1.89, -y)])
        box('Canopy seam', (0, y, 3.08), (1.39, .045, .042), pale)
    vertices = [vertex for ring in canopy for vertex in ring]
    faces = [(i+j, i+j+1, i+j+7, i+j+6) for i in (0,6,12) for j in range(5)]
    perimeter.mesh('Stretched wagon canopy', vertices, faces, [canvas])
    for i, y in enumerate((-1.19, 1.19)):
        box('Wagon axle', (0, y, .61), (3.05, .14, .14), metal)
        for side, label in [(-1, 'L'), (1, 'R')]:
            wheel = local_parent(f'Wagon Wheel {label}{i}', (side*1.47, y, .63))
            bpy.ops.mesh.primitive_torus_add(major_segments=12, minor_segments=6,
                                            major_radius=.50, minor_radius=.13)
            rim = bpy.context.object
            rim.name = 'Cleated wheel rim'
            rim.rotation_euler.y = math.pi/2
            rim.parent = wheel
            rim.data.materials.append(metal)
            for angle in range(4):
                spoke = box('Wheel crosspiece', (0, 0, 0), (.17, 1.02, .075), wood)
                spoke.rotation_euler.x = angle*math.pi/4
                spoke.parent = wheel
    for x in (-.52, .52):
        box('Draught pole', (x, -2.61, 1.09), (.10, 2.26, .10), wood)
        box('Wagon lantern frame', (x*1.75, 1.92, 2.07), (.26, .22, .42), metal)
        box('Wagon lantern glass', (x*1.75, 2.05, 2.07), (.17, .12, .30), amber)
    ico('Canvas bedroll', (.47, 1.37, 1.60), (.34, .42, .29), pale, 1)
