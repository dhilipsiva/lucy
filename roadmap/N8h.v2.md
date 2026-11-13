# N8h revision — can a headset camera run during mixed reality?

His Meta Quest 3 report (N8h-evidence.json, 7b6a701) granted planes, meshes, GPU depth,
hit-test and anchors in immersive-ar, but not WebXR camera access; a plain 2D page did open
one headset camera at 640x480. Capturing his home with textures (his request of 2026-11-14)
depends on one more fact: whether that getUserMedia camera keeps delivering frames while an
immersive-ar session runs, so each picture can be paired with his head pose.

Before implementation, the checks are: a new button asks for the 2D camera and the
immersive-ar session in the same activation; during the session the page counts the frames
the camera delivers and, once, the mean brightness of one frame drawn at 32x24, so black
frames are told from real ones; the stream stops when the session ends. The report gains
these counts only: never a frame, a pixel array or an image. A feature is reported as
working only when frames arrived during the session with non-black content; a refusal, a
silent stream or black frames each say so. Native tests cover the new row, and the JSON
allowlist test grows by these keys only. The shim proves orchestration only.

Evidence goes to N8h-evidence.json from his headset's JSON. Welfare: the camera is opened
only on his press, nothing it sees is kept, and the session ends by itself. Using a camera
for capture remains with D2 and the capture slice's own pre-registration. Undo by reverting
the revision.
