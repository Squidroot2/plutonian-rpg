"""Build the northwestern worm nest hollow, burrow mounds, and scout cache."""

import math
import random

import bmesh
import bpy
from mathutils import Vector

from asset_helpers import box, cone, ico
from perimeter import mesh

SOLID_PREFIX = 'Worm nest solid '
TRIM_PREFIX = 'Worm nest trim '

# Bevy coordinates for the 3 escarpments relocated to frame the hidden nest entrance
# and northern shoulder instead of plugging the middle of the western shelf.
ESCARPMENT_MOVES = {
    'Ice escarpment.048': (-29.0, 60.5, 1.20),
    'Ice escarpment.056': (-45.5, 51.0, 1.20),
    'Ice escarpment.015': (-50.5, 80.5, 1.20),
}

# (Bevy x, Bevy z, outer radius, rim height, yaw_deg)
BURROW_SITES = (
    (-57.0, -61.0, 2.5, 1.45, 25.0),
    (-59.0, -70.5, 2.7, 1.60, -15.0),
    (-50.5, -75.5, 2.6, 1.50, -75.0),
    (-41.5, -73.0, 2.4, 1.35, -130.0),
    (-44.0, -58.5, 2.3, 1.30, 110.0),
)

CENTRAL_ROCK_SITE = (-47.5, -66.5)
SCOUT_CRATE_SITE = (-55.5, -66.5)
SCOUT_LANTERN_SITE = (-54.85, -67.25)

# West to east in Bevy coordinates: x, south foot z, crest z, north foot z, crest y.
# The eastern tip lets explorers round the ridge on the way west from the core trail.
SOUTH_RIDGE_PROFILES = (
    (-47.0, -43.0, -46.0, -48.0, 4.5),
    (-42.0, -42.0, -46.5, -49.0, 7.0),
    (-36.0, -41.0, -46.0, -49.0, 6.2),
    (-30.0, -40.5, -44.0, -48.0, 7.4),
    (-24.0, -41.5, -45.0, -48.5, 5.5),
    (-19.0, -43.0, -46.0, -49.0, 5.7),
    (-15.0, -45.0, -46.5, -48.0, 4.5),
    (-12.5, -46.0, -47.0, -47.5, 0.6),
)


def is_worm_nest(name):
    return name.startswith((SOLID_PREFIX, TRIM_PREFIX))


def clear():
    for obj in list(bpy.context.scene.objects):
        if is_worm_nest(obj.name):
            data = obj.data
            bpy.data.objects.remove(obj, do_unlink=True)
            if data is not None and data.users == 0:
                bpy.data.meshes.remove(data)


def widen_shelf():
    shelf = bpy.data.objects['Ice shelf']
    changed = False
    for vertex in shelf.data.vertices:
        if vertex.co.x < 0.0 and vertex.co.x > -71.9:
            vertex.co.x = -72.0
            changed = True
    if changed:
        shelf.data.update()


def reposition_escarpments():
    for name, location in ESCARPMENT_MOVES.items():
        obj = bpy.data.objects.get(name)
        if obj is not None:
            obj.location = location


def _shelf_height(shelf, inv, down, x, y):
    hit, loc, _, _ = shelf.ray_cast(inv @ Vector((x, y, 20.0)), down)
    if not hit:
        raise RuntimeError(f'Worm nest point ({x:.2f}, {y:.2f}) missed Ice shelf')
    return (shelf.matrix_world @ loc).z


def _south_ridge(materials):
    """Screen the nest from settlement cameras with a continuous faceted ridge."""
    vertices = []
    for x, south, crest, north, height in SOUTH_RIDGE_PROFILES:
        vertices.extend([
            (x, -.8, south),
            (x, height * .58, south + (crest - south) * .40),
            (x, height, crest),
            (x, height * .73, north + (crest - north) * .50),
            (x, -.8, north),
        ])

    width = 5
    faces = [tuple(range(width))]
    for start in range(0, len(vertices) - width, width):
        for side in range(width):
            a = start + side
            b = start + (side + 1) % width
            faces.extend([(a, a + width, b), (b, a + width, b + width)])
    faces.append(tuple(reversed(range(len(vertices) - width, len(vertices)))))
    ridge = mesh(f'{SOLID_PREFIX}south ridge', vertices, faces, materials)
    for face in ridge.data.polygons[1:-1]:
        segment, facet = divmod(face.index - 1, width * 2)
        face.material_index = (facet // 2 + segment % 2) % len(materials)


def _burrow_mound(name, cx, cy, gz, radius, height, yaw_deg, materials, rng):
    segments = 10
    yaw = math.radians(yaw_deg)
    # Profile from outer skirt -> mid slope -> jagged crest -> inner throat -> recessed floor.
    rings = [
        (radius, -0.18, 0.14),
        (radius * 0.72, height * 0.56, 0.16),
        (radius * 0.48, height, 0.22),
        (radius * 0.34, height * 0.42, 0.10),
        (radius * 0.24, -0.22, 0.05),
    ]
    vertices = []
    for ring_idx, (r_base, z_base, jitter) in enumerate(rings):
        for seg in range(segments):
            angle = math.tau * seg / segments + yaw
            # Dip one side slightly to suggest a worm-entry lip facing into the nest.
            lip = 0.68 if ring_idx in (1, 2) and math.cos(angle - yaw) > 0.35 else 1.0
            r = r_base * (1.0 + rng.uniform(-jitter, jitter))
            z = gz + z_base * lip + rng.uniform(-0.06, 0.08) * (1 if ring_idx in (1, 2) else 0)
            vertices.append((cx + math.cos(angle) * r, cy + math.sin(angle) * r, z))

    faces = []
    for row in range(len(rings) - 1):
        for seg in range(segments):
            nxt = (seg + 1) % segments
            a = row * segments + seg
            b = row * segments + nxt
            c = (row + 1) * segments + nxt
            d = (row + 1) * segments + seg
            faces.append((a, b, c, d))
    # Cap the recessed inner floor so the mesh is watertight.
    inner_start = (len(rings) - 1) * segments
    faces.append(tuple(range(inner_start, inner_start + segments)))

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
    for face in mesh.polygons:
        if face.index >= (len(rings) - 2) * segments:
            face.material_index = 2  # Dark basalt throat
        elif face.index >= segments:
            face.material_index = 1 if face.index % 3 == 0 else 0
        else:
            face.material_index = 0
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def build():
    clear()
    bpy.context.view_layer.update()
    shelf = bpy.data.objects['Ice shelf']
    inv = shelf.matrix_world.inverted()
    down = inv.to_3x3() @ Vector((0, 0, -1))

    ice = bpy.data.materials['Blue graphite ice']
    frost = bpy.data.materials['Rim frost seams']
    basalt = bpy.data.materials['Basalt']
    crate_mat = bpy.data.materials['Shelter enamel green']
    frame_mat = bpy.data.materials['Shelter graphite frame']
    stripe_mat = bpy.data.materials['Shelter aisle edging']
    amber = bpy.data.materials['Amber habitation']
    rng = random.Random(310)

    _south_ridge((basalt, ice, frost))

    for index, (bx, bz, radius, height, yaw_deg) in enumerate(BURROW_SITES, start=1):
        cx, cy = bx, -bz
        gz = _shelf_height(shelf, inv, down, cx, cy)
        _burrow_mound(
            f'{SOLID_PREFIX}burrow mound {index}',
            cx, cy, gz, radius, height, yaw_deg,
            (ice, frost, basalt), rng,
        )
        # Decorative frost needles around the burrow lip.
        for shard_idx in range(3):
            angle = math.radians(yaw_deg) + (shard_idx - 1) * 1.15 + rng.uniform(-0.2, 0.2)
            sx = cx + math.cos(angle) * (radius * 0.60)
            sy = cy + math.sin(angle) * (radius * 0.60)
            sz = _shelf_height(shelf, inv, down, sx, sy)
            shard_h = rng.uniform(0.9, 1.35)
            shard = cone(
                f'{TRIM_PREFIX}ice shard {index}_{shard_idx}',
                (sx, sy, sz + height * 0.45 + shard_h * 0.35),
                rng.uniform(0.16, 0.25),
                shard_h,
                frost,
            )
            shard.rotation_euler = (rng.uniform(-0.35, 0.35), rng.uniform(-0.35, 0.35), 0.0)

    # Central faceted rock pillar for kiting and line-of-sight breaks.
    rx, ry = CENTRAL_ROCK_SITE[0], -CENTRAL_ROCK_SITE[1]
    rz = _shelf_height(shelf, inv, down, rx, ry)
    rock = ico(f'{SOLID_PREFIX}central rock', (rx, ry, rz + 0.65), (1.35, 1.25, 1.10), basalt, 1)
    rock.data.materials.append(frost)
    for vertex in rock.data.vertices:
        vertex.co.x *= rng.uniform(0.84, 1.12)
        vertex.co.y *= rng.uniform(0.84, 1.12)
    rock.data.update()
    for poly in rock.data.polygons:
        if poly.normal.z > 0.35:
            poly.material_index = 1

    # Abandoned scout supply crate and fallen lantern at the far back of the nest.
    kx, ky = SCOUT_CRATE_SITE[0], -SCOUT_CRATE_SITE[1]
    kz = _shelf_height(shelf, inv, down, kx, ky)
    crate = box(f'{SOLID_PREFIX}scout crate', (kx, ky, kz + 0.26), (1.05, 0.76, 0.58), crate_mat)
    crate.rotation_euler = (0.08, -0.10, 0.32)
    crate_rot = crate.rotation_euler.to_matrix()
    lid_offset = crate_rot @ Vector((0.0, 0.0, 0.27))
    lid = box(
        f'{TRIM_PREFIX}scout crate lid',
        (kx + lid_offset.x, ky + lid_offset.y, kz + 0.26 + lid_offset.z),
        (1.10, 0.81, 0.09),
        frame_mat,
    )
    lid.rotation_euler = crate.rotation_euler
    for side in (-1, 1):
        latch_offset = crate_rot @ Vector((side * 0.28, -0.39, 0.20))
        latch = box(
            f'{TRIM_PREFIX}scout crate latch {side}',
            (kx + latch_offset.x, ky + latch_offset.y, kz + 0.26 + latch_offset.z),
            (0.09, 0.05, 0.12),
            stripe_mat,
        )
        latch.rotation_euler = crate.rotation_euler

    lx, ly = SCOUT_LANTERN_SITE[0], -SCOUT_LANTERN_SITE[1]
    lz = _shelf_height(shelf, inv, down, lx, ly)
    base = cone(f'{TRIM_PREFIX}scout lantern base', (lx, ly, lz + 0.08), 0.16, 0.14, basalt, 0.09)
    base.rotation_euler = (0.38, 0.18, 0.2)
    crystal = ico(f'{TRIM_PREFIX}scout lantern crystal', (lx, ly - 0.06, lz + 0.22), (0.10, 0.10, 0.18), amber)
    crystal.rotation_euler = base.rotation_euler
