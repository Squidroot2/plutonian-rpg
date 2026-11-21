"""Render the title backdrop from art/outpost.blend without modifying the source."""

from pathlib import Path

import bpy
from mathutils import Vector


def aim(obj, destination):
    obj.rotation_euler = (Vector(destination) - obj.location).to_track_quat('-Z', 'Y').to_euler()


scene = bpy.context.scene
bpy.ops.object.camera_add(location=(10, -25, 6.0))
camera = bpy.context.object
aim(camera, (-6, -2, 5.5))
camera.data.lens = 37
scene.camera = camera

charon = bpy.data.objects['Charon']
charon.location = (-21, 79, 25)
charon.scale *= .53

world = bpy.data.worlds.new('Plutonian twilight')
world.use_nodes = True
world.node_tree.nodes['Background'].inputs['Color'].default_value = (.027, .044, .088, 1)
world.node_tree.nodes['Background'].inputs['Strength'].default_value = .36
scene.world = world

bpy.ops.object.light_add(type='SUN', location=(12, -18, 28))
sun = bpy.context.object
aim(sun, (0, 2, 0))
sun.data.energy = 1.35
sun.data.angle = .12
sun.data.color = (.44, .64, 1.0)

for position, color, power, size in [
    ((2, -13, 4), (.31, .65, 1.0), 2300, 8),
    ((1, -4, 5.8), (.17, .78, 1.0), 1450, 3),
    ((-10, -5, 2), (1.0, .42, .10), 1200, 2),
]:
    bpy.ops.object.light_add(type='AREA', location=position)
    lamp = bpy.context.object
    aim(lamp, (position[0], position[1] + 1, 0))
    lamp.data.color = color
    lamp.data.energy = power
    lamp.data.size = size

ice = bpy.data.materials['Blue graphite ice']
nodes = ice.node_tree.nodes
links = ice.node_tree.links
surface = nodes.get('Principled BSDF')
noise = nodes.new('ShaderNodeTexNoise')
noise.inputs['Scale'].default_value = 4.0
noise.inputs['Detail'].default_value = 4.0
coordinates = nodes.new('ShaderNodeTexCoord')
links.new(coordinates.outputs['Object'], noise.inputs['Vector'])
bump = nodes.new('ShaderNodeBump')
bump.inputs['Strength'].default_value = .28
bump.inputs['Distance'].default_value = .12
links.new(noise.outputs['Fac'], bump.inputs['Height'])
links.new(bump.outputs['Normal'], surface.inputs['Normal'])

scene.render.engine = 'CYCLES'
scene.cycles.samples = 24
scene.cycles.use_denoising = True
scene.render.resolution_x = 1600
scene.render.resolution_y = 900
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = 'PNG'
scene.render.image_settings.color_mode = 'RGB'
output = Path(__file__).resolve().parents[1] / 'assets/ui/title.png'
output.parent.mkdir(parents=True, exist_ok=True)
scene.render.filepath = str(output)
bpy.ops.render.render(write_still=True)
