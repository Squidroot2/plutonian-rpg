"""Ground assembled outpost props against the authored ice shelf in Blender."""
from pathlib import Path

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[1]

EMBED = 0.04  # Bury flat foundations slightly to close gaps on the inclined ice.


def bounds(obj):
    return [obj.matrix_world @ Vector(corner) for corner in obj.bound_box]


def support_groups():
    objects = list(bpy.context.scene.objects)
    groups = []
    habitats = [o for o in objects if o.name.startswith('Habitat ') and not o.name.startswith('Habitat cap')]
    for body in habitats:
        attached = [body]
        for obj in objects:
            if obj.name.startswith(('Habitat cap', 'Lit doorway', 'Window')):
                nearest = min(habitats, key=lambda h: (h.location.xy-obj.location.xy).length_squared)
                if nearest == body:
                    attached.append(obj)
        groups.append((body, attached))
    base = bpy.data.objects['Beacon foundation']
    groups.append((base, [o for o in objects if o.name.startswith('Beacon ')]))
    stones = [o for o in objects if o.name.startswith('Waystone') and not o.name.startswith('Waystone lamp')]
    for stone in stones:
        lamps = [o for o in objects if o.name.startswith('Waystone lamp')
                 and min(stones, key=lambda s: (s.location.xy-o.location.xy).length_squared) == stone]
        groups.append((stone, [stone, *lamps]))
    for obj in objects:
        if obj.name.startswith(('Core plinth', 'Cave crystal')):
            groups.append((obj, [obj]))
    return groups


def ground_outpost(check_only=False):
    bpy.context.view_layer.update()
    shelf = bpy.data.objects['Ice shelf']
    tree = BVHTree.FromPolygons([shelf.matrix_world @ v.co for v in shelf.data.vertices],
                                [list(p.vertices) for p in shelf.data.polygons])
    changes = {}
    gaps = []
    for anchor, objects in support_groups():
        corners = bounds(anchor)
        supports = [tree.ray_cast(Vector((p.x, p.y, 1000)), Vector((0, 0, -1)))[0] for p in corners]
        if any(p is None for p in supports):
            raise RuntimeError(f'{anchor.name} is outside the ice shelf')
        ground = min(p.z for p in supports)
        bottom = min(p.z for p in corners)
        gap = bottom-ground
        gaps.append((anchor.name, gap))
        if check_only:
            if abs(gap+EMBED) > 0.002:
                raise AssertionError(f'{anchor.name}: foundation offset {gap:.3f} m, expected {-EMBED:.3f}')
            continue
        delta = ground-EMBED-bottom
        for obj in objects:
            matrix = obj.matrix_world.copy()
            matrix.translation.z += delta
            obj.matrix_world = matrix
            changes[obj.name] = delta
    bpy.context.view_layer.update()
    if check_only:
        print(f'Ground contact OK: {len(gaps)} foundations')
    else:
        floating = [(name, round(gap, 3)) for name, gap in gaps if gap > .02]
        print(f'Closed foundation gaps (meters): {floating}')
    return changes


def check_collision_alignment():
    # Check placement without replacing authored collision simplifications.
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/outpost.blend'))
    ground_outpost(check_only=True)
    visual = {o.name: o.matrix_world.copy() for o in bpy.context.scene.objects if o.type == 'MESH'}
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'art/collision.blend'))
    for obj in bpy.context.scene.objects:
        if obj.name in visual:
            error = max(abs(a-b) for row_a, row_b in zip(obj.matrix_world, visual[obj.name]) for a, b in zip(row_a, row_b))
            assert error < 0.0001, f'{obj.name}: visual/collision transforms differ'
    print('Visual/collision transforms aligned')


if __name__ == '__main__':
    check_collision_alignment()
