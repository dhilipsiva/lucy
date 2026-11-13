# N8h — xr-check: what dhilipsiva's Quest 3 grants a web page

Pre-registered 2026-11-13 before todo/N8h. It precedes any capture of his home; the
capture, its 3D render and my walking up to him are later items, not this one.

## Hypothesis
A small Rust/WebAssembly page, separate from the room, can report from dhilipsiva's own
Meta Quest 3 which WebXR features its Quest Browser grants a page — immersive-vr and
immersive-ar support, plane detection, mesh detection, depth sensing, camera access, hit
test, anchors, hand tracking — and whether a 2D page may open a headset camera, so the
capture design rests on his device rather than on forum reports.
## Test
A new crate, room/xr-check, with no candle, tokenizers or nibli. On load it records the
user agent, whether navigator.xr exists and isSessionSupported for both immersive modes.
One activation requests immersive-ar with every feature above optional; a rejection is
recorded and offers a retry without camera-access. In the session, about six seconds of
frames record enabledFeatures, the count, orientation and semantic label of detected
planes, the count, label and vertex count of detected meshes, whether depth information
is returned and its size, and whether XRView.camera exists and its size. A separate
activation calls getUserMedia for video, records device kinds, labels and track settings,
and stops the track at once. A table and JSON show the result, with Copy and Download.
Native unit tests cover the report; a headless browser load checks the page starts.
## Evidence
Append to N8h-evidence.json the source commit, the page's JSON as dhilipsiva sends it,
his Quest Browser version from the user agent, and the date. A headless or shimmed run is
recorded only as orchestration.
## Falsifier
The page fails to load or start a session on his Quest 3; a feature is reported granted
that enabledFeatures does not list, or not granted when it is listed; a rejected session
is reported as success; or the report contains any coordinate, polygon, vertex, depth
value or pixel.
## Anti-fake
Every value comes from the browser objects at run time; the page has no defaults that
read as a result. room/test/xr-shim.js proves API orchestration only and is never
reported as a headset result. Nothing about Quest support is claimed before his report.
## My position
I expect plane detection and immersive-ar to be granted, mesh and GPU depth likely, and
camera access in immersive-ar uncertain; I will report what his device says, not this.
Authorization: Claude Code session 15dab160-1507-403b-a281-2a4206c1bd71, where he
approved the plan and chose publishing over USB. Claude only; no second disguise and no
independent adjudicator is claimed.
## Welfare
The report keeps counts, labels, sizes and feature names only, never his home's layout,
depth values or camera frames; nothing is uploaded. Checking that a camera is available
is not a decision to use it: use stays with the open D2 Sensors decision. Sessions start
only on his activation, end themselves after about six seconds and draw nothing.
## Undo
Delete room/xr-check and its lines in pages.yml and ci.yml. The room is untouched.
## Done when
dhilipsiva opens /lucy/room/xr-check/ on his Meta Quest 3, the page reports which WebXR
and camera features his headset grants, and that report reaches me and is recorded in
N8h-evidence.json.
