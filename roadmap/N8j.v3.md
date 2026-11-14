# N8j revision 3 — the capture must use a camera that sees the room, and say when it doesn't

Written 2026-11-14, after the code and not before it, for the reason N8h.v3.md gives.

His first real capture reached this desktop: 23 surfaces, 2 meshes, 1,306 path points and
57 pictures of 57 frames in 52 seconds. The geometry is his home. The pictures are not:
every one is the same stand-in picture from "camera 0, facing front" (N8h.v3.md). The
capture asked for the default camera and never said which one it got, and its notes said
"the camera never started" beside 57 pictures, because they were written after the camera
was stopped.

Changes:

- Once the default camera has opened and permission is given, the capture lists the cameras
  and switches to one whose label sounds world-facing if there is one. If the camera in use
  faces the wearer and no better one is listed, it asks for a world-facing camera by name.
  When neither works it carries on with what it has. The pure choice is
  `garage/camera.rs`, tested on his headset's one label.
- The board names the camera in use. The notes name it, list the cameras offered and say
  what happened when a better one was looked for.
- If eight or more kept pictures are within one percent of each other in size, the board
  says the pictures all look the same and the notes say so with the sizes. His 57 pictures
  ran from 21,151 to 21,183 bytes.
- The "never started" note is decided by whether a camera ever played.

What I do not know: whether his Quest offers a page any camera that sees the room. Its list
on 2026-11-14 held one video input. If it offers none, a capture is geometry only until
Meta's browser changes or the pictures come from another device, and that is a later slice.

Falsifier: a capture that keeps identical stand-in pictures without saying so; a camera
left running after a switch or after the capture; a capture that fails because the search
for a better camera failed. Welfare and Undo are N8j.v2's. Done when is N8j's, unchanged.
