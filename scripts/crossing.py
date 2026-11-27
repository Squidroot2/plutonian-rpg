"""Authored terrain and caravan models for the Listening Flats."""

import math
import random

import bpy
from mathutils import Vector

import perimeter
from asset_helpers import material, box, ico

MODELS = ['listening_flats', 'listening_flats_collision', 'nivalis_gate', 'vellik', 'crossing_wagon']
RIM = [
    (-25, 26.8), (-25, 45), (-37.5, 54), (-37, 73), (-43, 98),
    (-62, 112), (-111, 112), (-126, 97), (-128, 57), (-114, 38),
    (-77, 36), (-57, 45), (-43, 54), (-35, 48), (-35, 26.8),
]
FLOOR = -1.46


def block(name, at, size, mat):
    x, y, z = at
    width, height, depth = size
    return box(name, (x, -z, y), (width, depth, height), mat)


def flats():
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
            block('Flats solid Crossing waystone', (x+dx, FLOOR+.58, z+dz),
                  (.56, 1.2, .56), basalt)
            block('Crossing waystone glass', (x+dx, FLOOR+1.2, z+dz), (.28, .24, .28), pale)
    for x, z, sx, sz, height in [
        (-69, 50, 5, 4, 3.0), (-80, 57, 6, 4, 4.1), (-84, 81, 8, 5, 4.6),
        (-71, 85, 5, 4, 3.4), (-103, 58, 8, 8, 6.0), (-111, 68, 6, 4, 3.4),
        (-63, 98, 6, 4, 4.2), (-54, 74, 6, 4, 5.1), (-115, 100, 3, 3, 3.2),
        (-97, 103, 4, 4, 3.4), (-92, 47, 3, 3, 2.5), (-117, 58, 3, 3, 3.0),
    ]:
        shard = ico('Flats solid Ice fin', (x, -z, FLOOR+.1), (sx, sz, height),
                    rng.choice([ice, frost, basalt]), 1)
        shard.rotation_euler.y = rng.uniform(-.2, .2)

    cairn(basalt, bronze, pale)
    overlook(basalt, frost)


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


def cairn(basalt, bronze, pale):
    for i in range(5):
        radius = 2.05-i*.27
        ico('Flats solid Bell cairn', (-101, -86, FLOOR+.22+i*.40),
            (radius, radius*.85, .44), basalt, 1)
    for x in (-102.2, -99.8):
        block('Flats solid Bell frame post', (x, FLOOR+1.65, 85.3), (.26, 3.3, .26), bronze)
    block('Flats solid Bell frame bar', (-101, FLOOR+3.3, 85.3), (2.7, .21, .25), bronze)
    for i, x in enumerate((-101.78, -101.25, -100.70, -100.15)):
        block('Wagon bell tether', (x, FLOOR+3.03, 85.3), (.045, .55, .045), bronze)
        ico('Hanging wagon bell', (x, -85.3, FLOOR+2.73-(i%2)*.16), (.18, .17, .22), bronze, 1)
    block('Cairn lamp', (-101, FLOOR+2.10, 86.6), (.28, .27, .28), pale)


def overlook(basalt, frost):
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
        block('Flats solid Overlook pillars', (x, 10.43, 67.5), (.64, 2.0, .7), basalt)


def local_parent(name, at, parent=None):
    obj = bpy.data.objects.new(name, None)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = parent
    obj.location = at
    return obj


def vellik():
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


def wagon():
    alloy = material('Salvaged Relay alloy', (.105, .183, .222), metal=.55)
    tread = material('Cleated graphite alloy', (.062, .09, .119), metal=.34)
    shell = material('Fired basalt ceramic', (.182, .262, .309), metal=.10)
    enamel = material('Frost-glazed ceramic', (.255, .374, .405), metal=.12)
    cloth = material('Woven basalt shelter cloth', (.115, .148, .214))
    seams = material('Woven basalt binding', (.181, .235, .30))
    marker = material('Crossing marker glass', (.12, .68, .72), emission=1.25)
    amber = material('Wagon lantern glass', (1.0, .49, .15), emission=3.4)

    wagon_chassis(alloy, shell, enamel, marker, amber)
    wagon_canopy(alloy, cloth, seams)
    for i, y in enumerate((-1.19, 1.19)):
        box('Reinforced wagon axle', (0, y, .63), (2.94, .20, .19), alloy)
        for side, label in [(-1, 'L'), (1, 'R')]:
            wheel = local_parent(f'Wagon Wheel {label}{i}', (side*1.47, y, .63))
            wagon_wheel(wheel, side, alloy, tread, enamel)

    for x in (-.52, .52):
        box('Alloy draught rail', (x, -2.61, 1.09), (.14, 2.26, .115), alloy)
        box('Draught rail coupling', (x, -3.69, 1.09), (.19, .24, .16), enamel)
    box('Harness crossbar', (0, -3.57, 1.09), (1.18, .10, .085), alloy)
    pack = ico('Basalt-cloth bedroll', (.47, 1.25, 1.40), (.34, .44, .27), cloth, 1)
    pack.rotation_euler.z = -.18


def wagon_chassis(alloy, shell, enamel, marker, amber):
    box('Pressed alloy underframe', (0, 0, 1.01), (2.20, 3.63, .24), alloy)
    box('Fired ceramic cargo deck', (0, 0, 1.16), (2.25, 3.63, .105), shell)

    # The sheared shoulders and exposed seams give the ceramic panels their shape.
    profile = ((1.09, 1.08), (1.26, 1.37), (1.24, 1.77), (1.10, 1.91))
    for side in (-1, 1):
        for start, end in ((-1.77, -.62), (-.58, .58), (.62, 1.77)):
            corners = [(side*x, height, -y) for y in (start, end) for x, height in profile]
            faces = [(i, i+1, i+5, i+4) for i in range(3)]
            if side > 0:
                faces = [face[::-1] for face in faces]
            perimeter.mesh('Bevelled ceramic cargo panel', corners, faces,
                           (shell, enamel), [0, 0, 1])
        box('Alloy sill strip', (side*1.11, 0, 1.14), (.12, 3.62, .12), alloy)
        box('Ceramic cap rail', (side*1.09, 0, 1.89), (.19, 3.62, .078), enamel)
        for y in (-1.67, 1.67):
            box('Alloy corner stanchion', (side*1.11, y, 1.52), (.28, .22, .81), alloy)

    for y, outward in [(-1.80, -1), (1.80, 1)]:
        box('Pressed alloy end frame', (0, y, 1.44), (2.23, .16, .63), alloy)
        for x in (-.33, .33):
            box('Fired ceramic end plate', (x, y+outward*.093, 1.43),
                (.61, .058, .46), shell)
        box('Cast pull handle', (0, y+outward*.14, 1.55), (.23, .10, .07), enamel)
        for x in (-.85, .85):
            box('Recessed lantern bracket', (x, y+outward*.098, 1.86),
                (.29, .21, .42), alloy)
            box('Lantern slit', (x, y+outward*.21, 1.86),
                (.15, .035, .22), amber if outward > 0 else marker)


def wagon_canopy(alloy, cloth, seam):
    # Tension the same basalt fibre as the Plutonian cloaks over a folded alloy frame.
    profile = [(-1.17, 1.88), (-1.13, 2.45), (-.66, 2.87),
               (.66, 2.87), (1.13, 2.45), (1.17, 1.88)]
    stations = (-1.65, -.53, .53, 1.65)
    vertices = [(x, height, -y) for y in stations for x, height in profile]
    count = len(profile)
    faces = [(i+j, i+j+count, i+j+count+1, i+j+1)
             for i in range(0, len(vertices)-count, count) for j in range(count-1)]
    perimeter.mesh('Basalt fibre canopy', vertices, [face[::-1] for face in faces], [cloth])

    for y in stations:
        for a, b in zip(profile, profile[1:]):
            start, end = Vector((a[0], y, a[1])), Vector((b[0], y, b[1]))
            delta = end-start
            rib = box('Folded canopy rib', (start+end)*.5, (.048, .060, delta.length+.022),
                      alloy if abs(y) > 1 else seam)
            rib.rotation_euler = delta.to_track_quat('Z', 'Y').to_euler()


def wagon_wheel(wheel, side, alloy, tread, enamel):
    # Wheel geometry stays local to its original pivot so Bevy can still roll it.
    for name, x, radius, depth, sides, mat in [
        ('Forged wheel disc', 0, .53, .16, 12, alloy),
        ('Frost-glazed hub collar', side*.096, .32, .035, 8, enamel),
        ('Hex axle cap', side*.121, .13, .038, 6, alloy),
    ]:
        bpy.ops.mesh.primitive_cylinder_add(vertices=sides, radius=radius, depth=depth,
                                            location=(x, 0, 0), rotation=(0, math.pi/2, 0))
        part = bpy.context.object
        part.name = name
        part.parent = wheel
        part.data.materials.append(mat)

    # The darker tread wraps the wheel edge and leaves the inset disc exposed.
    stations = 12
    cross_section = [(-.099, .505), (-.099, .56), (.099, .56), (.099, .505)]
    vertices = [(x, radius*math.cos(angle), radius*math.sin(angle))
                for angle in (math.tau*i/stations for i in range(stations))
                for x, radius in cross_section]
    faces = []
    for i in range(stations):
        a, b = i*4, ((i+1) % stations)*4
        for j in range(4):
            k = (j+1) % 4
            faces.append((a+j, b+j, b+k, a+k))
    perimeter.mesh('Faceted tread band', vertices, faces, [tread]).parent = wheel

    for i in range(12):
        angle = math.tau*i/12
        cleat = box('Cast ice cleat', (0, -.586*math.sin(angle), .586*math.cos(angle)),
                    (.234, .162, .075), alloy)
        cleat.rotation_euler.x = angle
        cleat.parent = wheel