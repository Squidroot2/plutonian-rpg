"""Build Nivalis's outer cliffs and sealed gate in world coordinates."""

import math
import random

import bpy
from mathutils import Vector
from asset_helpers import material, box


SOLID_PREFIX = 'Perimeter solid '
TRIM_PREFIX = 'Perimeter trim '

# Bevy coordinates. The two profiles also define the start of the South Basin tunnel.
CAVE_MOUTH = (83.95, 14, 3.5, -1.83, 4.7)
CAVE_APPROACH = (89, 14, 3.3, -2.48, 4.6)
TUNNEL_RING = (
    (-1, 0), (-.37, 0), (.29, 0), (1, 0), (1.04, .38), (.81, .82),
    (.18, 1), (-.46, .94), (-.91, .69), (-1.03, .31),
)


def is_perimeter(name):
    return name.startswith((SOLID_PREFIX, TRIM_PREFIX))


def mesh(name, vertices, faces, materials, indices=None):
    data = bpy.data.meshes.new(name)
    data.from_pydata([(x, -z, y) for x, y, z in vertices], [], faces)
    data.update()
    for mat in materials:
        data.materials.append(mat)
    if indices:
        for face, index in zip(data.polygons, indices):
            face.material_index = index
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def sample_path(path):
    """Keep the corner vertices and subdivide long runs for the changing crest."""
    points = [Vector(point) for point in path]
    sampled = [points[0]]
    for start, end in zip(points, points[1:]):
        count = math.ceil((end-start).length / 8.0)
        sampled.extend(start.lerp(end, step/count) for step in range(1, count+1))
    return sampled


def outward_edge(a, b):
    direction = (b-a).normalized()
    return Vector((direction.y, -direction.x))


def cliff(name, path, materials, rng, depth_scale=None, crest_limit=None):
    """Use one welded, steep inside face and an irregular mountain profile behind it."""
    vertices, faces, indices = [], [], []
    points = sample_path(path)
    for index, point in enumerate(points):
        incoming = outward_edge(points[index-1], point) if index else None
        outgoing = outward_edge(point, points[index+1]) if index+1 < len(points) else None
        if incoming is None or outgoing is None:
            outward = outgoing if incoming is None else incoming
        else:
            # Offset in the bisector so the profiles meet without a wedge at bends.
            outward = (incoming+outgoing) / (1+incoming.dot(outgoing))
        peak = rng.uniform(18, 31)
        if crest_limit is not None:
            peak = min(peak, crest_limit(point))
        profile = [
            (0, -8), (0, rng.uniform(4, 9)), (1.1, peak*.69),
            (rng.uniform(2.2, 4.4), peak),
            (rng.uniform(7, 11), peak*rng.uniform(.72, .96)),
            (rng.uniform(14, 19), rng.uniform(1, 6)), (17, -8),
        ]
        scale = depth_scale(point) if depth_scale is not None else 1.0
        for setback, height in profile:
            x, z = point + outward*setback*scale
            vertices.append((x, height, z))

    width = len(profile)
    faces.append(tuple(range(width-1, -1, -1)))
    indices.append(0)
    for start in range(0, len(vertices)-width, width):
        for j in range(width):
            a, b = start+j, start+(j+1) % width
            faces.extend([(a, b, a+width), (b, b+width, a+width)])
            choices = (0, 0, 1, 1, 2) if 1 <= j <= 4 else (0, 0, 1)
            indices.extend(rng.choices(choices, k=2))
    faces.append(tuple(range(len(vertices)-width, len(vertices))))
    indices.append(0)
    return mesh(SOLID_PREFIX + name, vertices, [face[::-1] for face in faces], materials, indices)


def cave_collar(materials):
    """Collide around the authored cave opening, overlapping the tunnel shell."""
    outer = [
        (7.6, -8), (12, -8), (16, -8), (20.4, -8), (20.4, -.6),
        (20.4, 16), (16, 23), (11.5, 20), (7.6, 15), (7.6, 0),
    ]
    vertices = []
    for depth in (83.85, 85.15):
        t = (depth-CAVE_MOUTH[0]) / (CAVE_APPROACH[0]-CAVE_MOUTH[0])
        center, width, floor, height = [
            a+(b-a)*t for a, b in zip(CAVE_MOUTH[1:], CAVE_APPROACH[1:])
        ]
        vertices.extend((x, y, depth) for x, y in outer)
        vertices.extend(
            (center+dx*(width-.055), floor+v*(height-.055)-.025, depth)
            for dx, v in TUNNEL_RING
        )

    count = len(outer)
    faces, indices = [], []
    for i in range(count):
        j = (i+1) % count
        front = (i, j, j+count, i+count)
        back = tuple(k+2*count for k in front)
        for strip, material in [
            (front, [0, 1, 0, 1, 2][i % 5]),
            (back[::-1], 0),
            ((front[0], back[0], back[1], front[1]), 0),
            ((front[2], back[2], back[3], front[3]), 1),
        ]:
            a, b, c, d = strip
            faces.extend([(a, b, c), (a, c, d)])
            indices.extend([material]*2)
    return mesh(SOLID_PREFIX + 'Cavern headwall', vertices, [face[::-1] for face in faces],
                materials, indices)


def gate(box, materials):
    ceramic, alloy, amber = materials

    def part(name, at, size, mat, solid=True):
        x, y, z = at
        width, height, depth = size
        return box((SOLID_PREFIX if solid else TRIM_PREFIX) + name,
                   (x, -z, y), (width, depth, height), mat)

    center = -30
    for side, label in [(-1, 'west'), (1, 'east')]:
        x = center+side*6.55
        part(f'Gate {label} buttress', (x, 3.1, 27.65), (4.3, 12.4, 5.1), ceramic)
        part(f'Gate {label} infill', (center+side*7.35, 5.4, 28.2),
             (4.1, 18.4, 3.4), ceramic)
        part(f'Gate {label} jamb', (center+side*5.12, 2.45, 26.2),
             (.75, 9.9, 2.2), alloy)
        part('Gate lamp housing', (x, 3.6, 25.03), (.8, 3.5, .32), alloy, False)
        part('Gate amber lamp', (x, 3.6, 24.85), (.19, 2.3, .10), amber, False)
    part('Gate header', (center, 8.45, 27.0), (13.4, 4.2, 4.0), ceramic)
    part('Gate header trim', (center, 7.06, 24.91), (11.0, .28, .22), alloy, False)
    for offset in (-1.0, 0, 1.0):
        part('Gate lintel lamp', (center+offset, 8.85, 24.95), (.24, .34, .12), amber, False)

    part('Gate lift housing', (center, 12.9, 27.0), (11.5, 5.0, 3.4), ceramic)
    for side in (-1, 1):
        part('Gate lift rail', (center+side*4.85, 12.7, 25.22), (.26, 4.6, .2), alloy, False)


def gate_leaf(box, materials):
    ceramic, alloy, enamel = materials
    leaf = box('Nivalis gate leaf', (0, 0, 0), (10.05, 1.10, 8.7), alloy)
    for offset in (-4.0, -2.0, 0, 2.0, 4.0):
        panel = box('Gate leaf panel', (offset, .59, .05), (1.68, .12, 7.35), enamel)
        panel.parent = leaf
    for height in (-1.3, 2.0):
        band = box('Gate leaf band', (0, .67, height), (9.7, .14, .20), ceramic)
        band.parent = leaf


def clear():
    for obj in list(bpy.context.scene.objects):
        if is_perimeter(obj.name):
            data = obj.data
            bpy.data.objects.remove(obj, do_unlink=True)
            if data.users == 0:
                bpy.data.meshes.remove(data)
    for mat in list(bpy.data.materials):
        if mat.name.startswith('Rim ') and mat.users == 0:
            bpy.data.materials.remove(mat)


def build():
    clear()
    rock = material('Rim basalt', (.045, .060, .088))
    ice = material('Rim blue ice', (.079, .137, .197), metal=.25)
    frost = material('Rim frost seams', (.115, .185, .237), metal=.2)
    rng = random.Random(137)

    # Follow the playable shelf on the inside and build the ridges outward from it.
    # The southwestern wall and the cavern collar connect the ends of the cliffs.
    shelf = [
        (-37.3, 27), (-46, 27), (-46.5, 8), (-45.5, -18), (-47, -48),
        (-46, -75), (-47, -108), (-46, -125), (-40, -130.7), (-13, -130.8),
        (13, -130.8), (40, -130.7), (46, -125), (47, -104), (46.5, -78),
        (47, -50), (46, -22), (47, 9), (46, 27), (19.3, 27),
    ]
    east = [
        (19.3, 27), (20.2, 30), (26.3, 41), (30.2, 51), (30.2, 62),
        (29.2, 74), (27.2, 83.85), (19.7, 83.85),
    ]
    west = [
        (8.3, 83.85), (-15.3, 83.85), (-23.2, 74), (-26.2, 62),
        (-28.2, 51), (-25.2, 41), (-20.2, 30), (-19.3, 27), (-22.7, 27),
    ]
    materials = (rock, ice, frost)
    cliff('Shelf cliffs', shelf+east[1:], materials, rng)
    # Leave a clear crossing road behind the basin rim, before the road bends west.
    cliff('Basin west cliffs', west, materials, rng,
          depth_scale=lambda point: .18+.82*max(0, min(1, (point.y-50)/19)))
    cave_collar(materials)
    gate(box, (
        material('Rim ceramic', (.096, .122, .153), metal=.25),
        material('Rim aged alloy', (.169, .237, .268), metal=.46),
        material('Rim amber signal', (.95, .36, .10), emission=3.0),
    ))