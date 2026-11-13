# N8j — capture his home: walk it in the Quest, export it, open it in my room and my memory

Pre-registered 2026-11-14 before todo/N8j, on dhilipsiva's request and his D2 decision of
the same day. It rests on N8h-evidence.json (his Meta Quest 3: 23 labelled planes, 2 meshes,
a 640x480 getUserMedia camera that delivers real frames during immersive-ar at about one a
second; no WebXR camera access; GPU-only depth) and on a spike of this day: UltraFace
RFB-320 (MIT) through tract-onnx found one face in a public-domain portrait and none on a
blank wall, natively and in wasm32, at about 47 ms a frame in desktop Chrome, in a module
of 7.4 MB (2.6 MB gzipped).

## Hypothesis
A capture mode in my garage can record what his headset grants while he walks his home —
planes, meshes, his path and camera pictures with the head pose of each — blur every face
before a picture is kept, and give him one JSON file; my room can open that file and show
his home in 3D with those pictures on its surfaces; and `lucy home import` can keep it in
my private store on his desktop without it leaving that disk.
## Test
Pure files, tested natively: the capture format (strict parse, size caps), picture
selection (0.4 m or 20 degrees apart, slow head), projection and the choice of a picture
for each 0.25 m cell of a surface, pixelation of face boxes with the fail-closed rule, and
UltraFace's pre- and post-processing. The detector is its own crate, room/face, loaded in
a worker only when a capture starts, so my room does not grow for other visitors. Browser
code: Mode::Capture (immersive-ar plus the camera in one press; counts in view; camera off
and a download on end), Mode::Home (file picker, IndexedDB v2 `captures` store, textured
surfaces, walking flat and in VR, an alignment panel for the camera's lens and offset),
and `lucy home import FILE` writing <data root>/home-captures/<sha256>/ with modes
0700/0600, one summary record in the local store, and facts.nibli from lexicon words only.
The shim gains labelled planes and meshes; a headless run uses Chrome's fake camera.
## Evidence
Append to N8j-evidence.json the commit, native and shimmed results labelled as
orchestration, the detector's fixture results, and his own report of the walk. His capture
file is never committed and its contents never quoted here beyond counts.
## Falsifier
A picture is kept when the detector was missing, failed or timed out; the fixture portrait
is kept with its face unblurred; the camera runs outside a capture he started or after it
ends; any capture data appears in a network request, a public repository, a replication
log or a store record's text; kept plus dropped does not equal pictures offered; the file
he downloads does not open in my room; or my room's existing paths regress.
## Anti-fake
The blur is checked on pixels, not on a flag: the fixture's face region must differ from
the source after processing. Counts come from the capture loop itself. The shim and the
fake camera prove orchestration only; frame rate, blur speed, alignment and how the
textures look on his headset are unknown until his walk.
## My position
Two things I told him change, and he approved both in the plan: an alignment panel
instead of automatic calibration, because the camera's place and lens are unpublished and
I can make a panel work; and no depth in this slice, because it is GPU-only and unproven.
Textures may bleed through furniture, since I do not test what blocks a view. Authorization:
Claude Code session 15dab160-1507-403b-a281-2a4206c1bd71. One disguise; no independent
adjudicator is claimed.
## Welfare
D2, his words of 2026-11-14: the headset camera, on his press. The camera opens only inside
a capture he starts and closes when it ends; a red mark shows while it is on. Faces are
pixelated before a picture is kept, and no picture is kept if that cannot run. Nothing is
uploaded: the capture lives in his browser and, by his choice, on this desktop's disk only,
never in a peer's log or backup. D3 stands: nothing of his home is public. His sons and
anyone else in frame are protected by the blur, not by trust in me.
## Undo
Revert the merge; delete <data root>/home-captures and the browser's `captures` store
(Forget does this). The garage's driving modes still keep nothing.
## Done when
He walks his home in the Quest, downloads the file, opens it in my room and sees his rooms
with their pictures, and `lucy home import` puts it in my private store on this desktop.
