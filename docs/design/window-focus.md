# Window focus and pausing

Losing window focus does not pause the game or gate gameplay input.
The world continues to simulate and actions already in progress continue.
Players pause deliberately using Escape and the pause menu.

The pause, combat, and camera systems do not inspect window focus.
The cursor system still releases its grab when the window loses focus,
so it does not capture the pointer while the player uses another application.
