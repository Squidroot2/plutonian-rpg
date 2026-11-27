"""Build Nivalis's inhabited ceramic shelters in the outpost scene."""

from dataclasses import dataclass
import math

import bpy
from mathutils import Vector

from asset_helpers import box, cone, ico, material

SOLID = 'Village solid '
TRIM = 'Village trim '


@dataclass(frozen=True)
class Home:
    name: str
    center: tuple
    inward: tuple
    width: float
    depth: float
    shoulder: float
    crown: float
    bevel: float
    cots: tuple


# Positions and orientation use the same X/Z coordinates as the game.
HOMES = (
    Home('Family', (-10, -1), (-1, 0), 7.2, 7.2, 2.9, 4.2, .9,
         ((-2.30, -.25), (2.30, -.25))),
    Home('Workshop', (-13, 8), (-1, 0), 6.6, 7.2, 3.2, 5.05, .61,
         ((-2.08, .32),)),
    Home('Orla', (12, 10), (1, 0), 5.7, 5.6, 2.72, 3.37, .72,
         ((-1.69, .24),)),
    Home('Keeper', (5.5, 17), (0, 1), 4.6, 4.4, 2.72, 4.25, .48,
         ((-1.27, .22),)),
    Home('Barracks', (-9, -10.15), (-1, 0), 6.8, 7.0, 3.04, 3.79, .46,
         ((-2.02, -.12), (2.02, -.12))),
)


def remove_homes():
    prefixes = ('Habitat ', 'Lit doorway', 'Window', SOLID, TRIM)
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith(prefixes):
            bpy.data.objects.remove(obj, do_unlink=True)


class Shelter:
    def __init__(self, home, palette):
        self.home = home
        self.mat = palette
        self.box, self.ico, self.cone = box, ico, cone
        self.x, self.z = home.center
        self.inward = Vector((home.inward[0], -home.inward[1], 0))
        self.side = Vector((self.inward.y, -self.inward.x, 0))
        self.angle = math.atan2(self.side.y, self.side.x)
        shelf = bpy.data.objects['Ice shelf']
        bpy.context.view_layer.update()
        heights = []
        for u in (-home.width / 2, home.width / 2):
            for v in (-home.depth / 2, home.depth / 2):
                edge = self.point((u, v, 0), floor=0)
                probe = shelf.matrix_world.inverted() @ (edge + Vector((0, 0, 10)))
                direction = shelf.matrix_world.inverted().to_3x3() @ Vector((0, 0, -1))
                hit, point, _, _ = shelf.ray_cast(probe, direction)
                if not hit:
                    raise RuntimeError(f'{home.name} extends past the ice shelf')
                heights.append((shelf.matrix_world @ point).z)
        self.floor = max(heights) + .035

    def point(self, local, floor=None):
        u, v, h = local
        height = self.floor if floor is None else floor
        return Vector((self.x, -self.z, height + h)) + u * self.side + v * self.inward

    def name(self, part, solid=False):
        return f'{SOLID if solid else TRIM}{self.home.name} {part}'

    def block(self, part, pos, size, mat, solid=False, angle=0):
        obj = self.box(self.name(part, solid), self.point(pos), size, self.mat[mat])
        obj.rotation_euler.z = self.angle + angle
        return obj

    def crystal(self, part, pos, size, mat):
        return self.ico(self.name(part), self.point(pos), size, self.mat[mat], 1)

    def cup(self, part, pos, radius, height, mat, top=None):
        return self.cone(self.name(part), self.point(pos), radius, height, self.mat[mat],
                         radius if top is None else top)

    def beam(self, part, a, b, width, mat):
        start, end = self.point(a), self.point(b)
        direction = end - start
        beam = self.box(self.name(part), (start + end) / 2,
                        (width, width, direction.length + .015), self.mat[mat])
        beam.rotation_mode = 'QUATERNION'
        beam.rotation_quaternion = direction.to_track_quat('Z', 'Y')

    def panel(self, part, face, back, surface, lining):
        count = len(face)
        faces = [tuple(range(count)), tuple(reversed(range(count, 2 * count)))]
        for i in range(count):
            j = (i + 1) % count
            faces.append((i, i + count, j + count, j))
        name = self.name(part, solid=True)
        data = bpy.data.meshes.new(name)
        data.from_pydata([self.point(p) for p in (*face, *back)], [], faces)
        data.update()
        obj = bpy.data.objects.new(name, data)
        bpy.context.collection.objects.link(obj)
        data.materials.append(self.mat[surface])
        data.materials.append(self.mat[lining])
        data.polygons[1].material_index = 1

    def ring(self, h, inset=0, ratio=1, shift=0):
        w = self.home.width * ratio / 2 - inset
        d = self.home.depth * ratio / 2 - inset
        c = self.home.bevel * ratio
        return [(x + shift, v, h) for x, v in (
            (-w+c, -d), (w-c, -d), (w, -d+c), (w, d-c),
            (w-c, d), (-w+c, d), (-w, d-c), (-w, -d+c),
        )]

    def shell(self):
        h = self.home
        bottom = self.ring(0)
        self.panel('deck', bottom, self.ring(-.27), 'deck', 'frame')
        upper = self.ring(h.shoulder)
        inner = self.ring(0, .15)
        inner_upper = self.ring(h.shoulder, .15)
        for i in range(1, len(bottom)):
            j = (i + 1) % len(bottom)
            self.panel(f'wall {i}', [bottom[i], bottom[j], upper[j], upper[i]],
                       [inner[i], inner[j], inner_upper[j], inner_upper[i]],
                       'shell', 'lining')
            self.beam('inner rib', inner[i], inner_upper[i], .10, 'frame')
        self.hatch()
        ratio, shift = {
            'Family': (.50, 0), 'Workshop': (.40, .25), 'Orla': (.53, -.56),
            'Keeper': (.38, .20), 'Barracks': (.78, 0),
        }[h.name]
        eave = self.ring(h.shoulder, -.09)
        crest = self.ring(h.crown, ratio=ratio, shift=shift)
        inside_crest = self.ring(h.crown - .12, .11, ratio, shift)
        for i in range(len(eave)):
            j = (i + 1) % len(eave)
            self.panel(f'vault facet {i}', [eave[i], eave[j], crest[j], crest[i]],
                       [inner_upper[i], inner_upper[j], inside_crest[j], inside_crest[i]],
                       'basalt', 'lining')
            self.beam('eave band', eave[i], eave[j], .12, 'frame')
        self.panel('crown', crest, inside_crest, 'basalt', 'lining')

    def siding(self):
        for u in (-self.home.width / 2, self.home.width / 2):
            for v in (-1.22, 1.22):
                self.block('Alloy seam', (u, v, self.home.shoulder / 2),
                           (.075, .17, self.home.shoulder - .08), 'frame')
            sign = math.copysign(1, u)
            self.block('Viewport frame', (u + sign*.025, 0, 1.92),
                       (.12, 1.48, .56), 'frame')
            self.block('Recessed viewport', (u + sign*.094, 0, 1.94),
                       (.045, 1.21, .28), 'blue' if self.home.name == 'Keeper' else 'amber')
            for v in (-.41, 0, .41):
                self.block('Viewport grille', (u + sign*.124, v, 1.94),
                           (.079, .075, .32), 'lining')


    def hatch(self):
        h = self.home
        front = -h.depth / 2
        extent = h.width / 2 - h.bevel
        door = [(-.96, 0), (-.96, 1.96), (-.68, 2.47),
                (.68, 2.47), (.96, 1.96), (.96, 0)]

        def wall(part, profile):
            self.panel(part, [(u, front, y) for u, y in profile],
                       [(u, front+.15, y) for u, y in profile], 'shell', 'lining')

        wall('left jamb', [(-extent, 0), *door[:3], (-.68, h.shoulder),
                           (-extent, h.shoulder)])
        wall('right jamb', [door[-1], (extent, 0), (extent, h.shoulder),
                            (.68, h.shoulder), *door[3:5]])
        wall('lintel', [door[2], door[3], (.68, h.shoulder), (-.68, h.shoulder)])
        for (a, y), (b, z) in zip(door, door[1:]):
            self.beam('hatch collar', (a, front-.02, y), (b, front-.02, z), .13, 'frame')
        self.beam('hatch tracer', (-.46, front-.1, 2.49), (.46, front-.1, 2.49),
                  .047, 'amber')
        ramp = [(-.87, front-.92, -.20), (.87, front-.92, -.20),
                (.87, front+.04, 0), (-.87, front+.04, 0)]
        self.panel('threshold ramp', ramp, [(u, v, y-.09) for u, v, y in ramp],
                   'deck', 'frame')

    def cot(self, number, u, v):
        label = f'bed {number}'
        self.block(f'{label} frame', (u, v, .17), (.88, 2.27, .34), 'frame', solid=True)
        self.block(f'{label} mattress', (u, v, .41), (.86, 2.19, .15), 'mattress', solid=True)
        fabric = {'Family': 'rust', 'Workshop': 'ochre', 'Orla': 'violet',
                  'Keeper': 'teal', 'Barracks': 'mattress'}[self.home.name]
        self.block(f'{label} blanket', (u, v-.21, .505), (.89, 1.55, .04), fabric)
        self.block(f'{label} folded seam', (u, v+.47, .532), (.90, .12, .04), fabric)
        self.crystal(f'{label} headrest', (u, v+.81, .54), (.34, .24, .08), 'linen')

    def furnish(self):
        for index, (u, v) in enumerate(self.home.cots, 1):
            self.cot(index, u, v)
        self.block('rear cabinet', (0, self.home.depth/2-.49, .34), (1.66, .57, .68),
                   'frame', solid=True)
        self.crystal('caged hearth glass', (.60, self.home.depth/2-.51, .92),
                     (.12, .12, .26), 'blue' if self.home.name == 'Keeper' else 'amber')
        getattr(self, self.home.name.lower())()

    def family(self):
        self.beam('Tack rail', (-1.45, 3.16, 1.35), (1.45, 3.16, 1.35), .095, 'frame')
        for i, u in enumerate((-.98, -.31, .36)):
            self.beam(f'Vellik harness strap {i}', (u, 3.12, 1.35),
                      (u-.14, 3.00, .88), .075, 'rust')
            self.crystal(f'Crossing marker {i}', (u+.21, 3.05, 1.72),
                         (.12, .065, .17), 'teal')

    def workshop(self):
        self.block('inspection bench', (2.18, .63, .47), (.86, 1.91, .94), 'lining',
                   solid=True)
        self.block('inspection surface', (2.12, .62, .98), (.73, 1.60, .082), 'frame')
        for i in range(5):
            self.crystal(f'Sample lens {i}', (1.98, .06+i*.22, 1.07),
                         (.058, .054, .045), 'blue')
        self.beam('Caliper', (2.38, -.02, 1.05), (2.38, .46, 1.09), .055, 'linen')
        for i in range(3):
            self.block(f'Ceramic sounding tile {i}', (2.18, 1.11+i*.16, 1.08),
                       (.45, .13, .072), 'ochre', angle=.06*i)

    def orla(self):
        self.block('Woven wall hanging', (1.45, 2.66, 1.46), (.91, .055, 1.07), 'violet',
                   angle=-.46)
        for i in range(4):
            self.beam(f'Chime tine {i}', (1.69+i*.16, -.95, 2.55),
                      (1.72+i*.16, -.95, 2.30-i*.08), .052, 'linen')
        self.cup('Fired basalt cup', (-.33, 2.26, .78), .097, .20, 'teal', .12)
        self.cup('Fired stone saucer', (-.33, 2.26, .69), .17, .028, 'ochre')

    def keeper(self):
        for i in range(3):
            self.cup(f'Spare beacon lens housing {i}', (-.50+i*.34, 1.61, .75),
                     .125, .16, 'frame', .10)
            self.crystal(f'Spare beacon lens {i}', (-.50+i*.34, 1.61, .83),
                         (.105, .105, .041), 'blue')
        self.block('Faded relay relief', (.75, 2.10, 1.63), (.72, .11, .62), 'basalt')
        self.beam('Relief inlay', (.75, 2.03, 1.39), (.75, 2.03, 1.89), .049, 'teal')

    def barracks(self):
        self.beam('Pikes storage rail', (-2.6, 2.31, 1.43), (-1.15, 2.31, 1.43),
                  .10, 'frame')
        for i in range(2):
            u = -2.21+i*.64
            self.beam(f'Training pike {i}', (u, 2.49, .29), (u-.10, 2.36, 1.96),
                      .048, 'linen')
            self.crystal(f'Training pike head {i}', (u-.10, 2.36, 2.01),
                         (.092, .040, .17), 'frame')
        self.block('Repair kit', (.10, 2.81, .76), (.62, .25, .16), 'teal')
        self.cup('Oiling pot', (-.49, 2.75, .77), .125, .18, 'ochre', .094)


def build():
    palette = {
        'shell': bpy.data.materials['Ceramic shell'],
        'basalt': bpy.data.materials['Basalt'],
        'amber': bpy.data.materials['Amber habitation'],
        'blue': bpy.data.materials['Trail phosphor'],
    }
    for name, color, metal in (
        ('lining', (.125, .153, .17), .28), ('deck', (.074, .086, .093), .16),
        ('frame', (.054, .067, .082), .56), ('mattress', (.124, .148, .166), .0),
        ('linen', (.34, .33, .26), .0), ('rust', (.28, .10, .064), .0),
        ('ochre', (.33, .245, .111), .10), ('violet', (.205, .11, .235), .0),
        ('teal', (.075, .208, .212), .25),
    ):
        key = f'Village {name}'
        palette[name] = bpy.data.materials.get(key) or material(key, color, metal=metal)
    for home in HOMES:
        shelter = Shelter(home, palette)
        shelter.shell()
        shelter.siding()
        shelter.furnish()
    workshop = Shelter(HOMES[1], palette)
    # A low side lamp marks the turn from the parking road into the courtyard.
    workshop.cup('Approach lamp pedestal', (-4.60, -1.30, .43), .13, 1.05, 'frame', .075)
    workshop.crystal('Approach lamp glass', (-4.60, -1.30, 1.10),
                     (.115, .115, .21), 'amber')
