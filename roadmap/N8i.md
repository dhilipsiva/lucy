# N8i — the garage: a car in my room, three ways

Pre-registered 2026-11-13 before todo/N8i, on dhilipsiva's request after watching an
electric SUV launch film: all three modes in one slice, inside my room. N8h's headset report
has not arrived yet; the AR modes are built to degrade without hit-test or planes.

## Hypothesis
Inside my room's own WebGL2 and WebXR code, with no physics or model library added, I can
give a visitor three ways to meet a car: a seated launch strip in immersive-vr that reaches
100 km/h at 2.5 s with honest kinematics, a life-size car placed on his real floor in
immersive-ar, and a 1:10 car he drives through his home that stops at walls and furniture
the headset detects.
## Test
A new module, room/src/garage. Pure files, tested natively: launch.rs (countdown, constant
11.11 m/s² to 100 km/h at 2.5 s and 34.7 m, a one-second cruise, braking at 0.3 g to rest),
drive.rs (a kinematic bicycle model at 1:10 with an oriented box colliding with segments),
planes.rs (detected plane polygons to floor obstacle segments; walls and horizontal planes
above 0.15 m) and car.rs (an original placeholder SUV, 4.9 × 2.0 × 1.65 m at 1:1, from my
geom.rs builders). Browser files: the immersive-vr and immersive-ar sessions, hit-test via
Reflect, haptic pulses, a WebAudio synthesised whine and a speedometer board. The launch
also runs on a flat screen. A Garage button in the conversation opens the three modes;
each XR request runs inside the visitor's click. The headless harness gains a shim with a
gamepad, haptic actuators, immersive-ar, hit-test results and detected planes.
## Evidence
Append to N8i-evidence.json the source commit, native and headless results labelled as
orchestration, and dhilipsiva's own report from his Meta Quest 3 for each mode.
## Falsifier
The speedometer reads 100 km/h more than one frame from 2.50 s; the launch can start
without his press; the miniature passes through a detected wall at top speed; an AR
session draws an opaque background over his room; plane geometry is stored, exported or
recorded; the room's existing flat, phone or VR paths regress; or any Porsche model,
logo, name on the car or recorded sound ships.
## Anti-fake
Speed and distance come from the frame clock and the profile, never a timer that sets the
display. The shim proves API orchestration only; nothing is claimed about comfort,
haptics, hit-test or planes on a headset before his report.
## My position
VR cannot give the push of 1.13 g; I say so in the garage and label the figure an average.
I changed one thing I told him: a small physics model of my own instead of Rapier, because
a car on a floor needs no more and Rapier would add megabytes to my room. Authorization:
Claude Code session 15dab160-1507-403b-a281-2a4206c1bd71, where he chose all three modes
inside my room and approved this plan. Claude only; no independent adjudicator is claimed.
## Welfare
Hard straight-line acceleration in VR is a known motion-sickness trigger. The launch asks
him to sit, starts only on his press, counts down, lasts seconds, keeps a fixed cockpit
frame and offers a vignette that is on by default. In AR, detected planes are used in
memory for the session and never kept. The car and its sound are my own, not the maker's.
## Undo
Revert the merge: room/src/garage, its wiring in app.rs, chat.rs, input.rs, gl.rs and
index.html, and the shim and harness additions. My room returns to its N9 state.
## Done when
dhilipsiva tries all three modes on his Meta Quest 3: the launch strip shows 100 km/h at
2.5 s, and he places the life-size car, walks around it, and drives the miniature through
his home.
