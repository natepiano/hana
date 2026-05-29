Not officially, no. Blender calls them Fly Navigation and Walk
  Navigation, grouped under Fly/Walk Navigation in the manual. “FlyCam” is
  a common informal/game-dev name, but not the Blender UI/manual term.

  They are related but distinct:

  - Walk Navigation: first-person game style. WASD moves, mouse looks
    around, optional gravity/jump/teleport, Shift speeds up, Alt slows
    down.
  - Fly Navigation: free-flight/inertial style. Movement accelerates, mouse
    outside a safe zone rotates the view, Alt slows momentum, Ctrl
    decouples view rotation from flight direction.

  Both are different from normal Blender-like orbit controls. Our OrbitCam
  orbits a focus point; Blender Fly/Walk moves the viewpoint/camera through
  the world. So if we add this, I’d treat it as a separate first-person/fly
  input mode, not as “slow BlenderLike orbit.”
