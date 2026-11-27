"""Shared Blender primitives and GLB export helpers."""

from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[1]


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
