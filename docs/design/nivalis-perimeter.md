# Nivalis perimeter

Blue-ice and basalt cliffs wrap the ice shelf, core trail, and South Basin.
  The cliff crests vary in height and shape,
  while their steep inward faces rise above the player’s reach from adjacent terrain.
  Ceramic walls join the cliffs to the southwest gate,
  where amber lamps and metal panels carry through the outpost’s material palette.

The southwest gate sits beside the South Basin route and opens into the Listening Flats during Contain the Hushcaps.
  The leaf has its own scene and a kinematic collider,
  with the panels and bands parented to it so they rise with the leaf.
  The static frame and cliff wings surround the doorway.

The welded cliff runs follow the supported edges of each shelf.
  Their bases extend below the ground and overlap at the other solid meshes.
  At Hushcap Caverns, the rock headwall surrounds the tunnel’s entrance profile
  and overlaps the tunnel shell while the cave path continues inside.

The perimeter solids ship with the outpost collision scene,
  so the existing terrain loading step prepares them before play.
  The world smoke isolates the perimeter colliders for capsule sweeps
  and exercises the gate with the movement controller.
  The cave smoke covers the entrance reveal and round trip.
