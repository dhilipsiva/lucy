# N8h revision 3 — which camera, and does it show the room?

Written 2026-11-14, after the code and not before it: dhilipsiva asked me to build and
publish in one go once his first real capture came back, so this is a record of what I
decided, not a pre-registration in the strict sense.

N8h.v2 called his headset's camera frames "real" because their mean brightness was 187 of
255 and not black. His capture of 2026-11-14 (N8j-evidence.json) kept 57 pictures from that
camera, "camera 0, facing front", and they are one soft pastel picture with pink bubbles,
57 times: a stand-in the headset gives a page, not a view of his room. Bright is not real.
That reading in N8h-evidence.json is corrected by a new entry.

The check now does what would have caught it. "Check camera (2D)" opens every camera the
browser lists, each by itself, and then asks for a world-facing and a wearer-facing camera
by name, in case the headset keeps one off the list. For each it looks twice, a second and
a half apart, while he moves his head, and reports numbers only: brightness, how much the
picture varies across itself, and how much it changed between the two looks. A small
picture of each camera is drawn on the page for his eyes and is never put in the report;
the allowlist test grows by the new numeric keys only. A row says whether any camera's
picture moved; a picture that did not move is reported as not a view of the room, and one
that did is left for him to judge, because numbers cannot tell a room from an animation.

Falsifier: a picture, or anything a picture could be rebuilt from, in the JSON; a camera
left running after the check; a still picture reported as showing the room. Evidence goes
to N8h-evidence.json from his headset's JSON and from what he says the small pictures show.
Welfare: cameras open only on his press, each for about three seconds, and nothing they see
leaves the page. Undo by reverting the revision. Chrome's fake camera in
room/test/xr-check.mjs proves orchestration only.
