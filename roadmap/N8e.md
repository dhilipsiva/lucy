# N8e — phone, gamepad and WebXR controls

Pre-registered 2026-10-26 before todo/N8e, after the approved N8b and N8c1 slices.
The user's later instruction retires Nell immediately; its redirect is already implemented.

## Hypothesis
HUD-aware pointer hysteresis, bounded textures, resilient worker startup and one Rust
interface shared across phone sheets, gamepads and WebXR can make the room usable across
these inputs without changing which source supplies an answer.
## Test
Native tests of pointer jitter, focus traversal, ray-to-panel coordinates, snap-turn and
room bounds. Browser checks of a 390×844 phone, gamepad typing and submission, worker
failure, and a stereo WebXR shim tracing real WebGL clear and viewport calls. Run the
repository and desktop gates, room tests, Wasm build and existing browser regressions.
Physical Quest 3 and BLE checks remain separately identified acceptance evidence.
## Evidence
Append source commit, measured outputs and limitations to N8-evidence.json. A synthetic
XR pass cannot establish headset compatibility, latency or comfort.
## Falsifier
Jitter swallowing a control, a hidden or unreachable phone control, lost in-flight queries
on worker fallback, two clears in a stereo frame, or inability to exit VR or type there.
## Anti-fake
Inspect actual canvas hit regions, IndexedDB records and WebGL calls. Use a browser shim
only for API orchestration and math; do not report it as a physical Quest test.
## My position
I will reuse my drawn panels and attributed reply paths for every input. Authorization is
in Codex session 01a178aa-3734-7f80-a70f-dd139859b6a7; the user's Nell retirement correction
supersedes the older retention condition. No independent adjudicator is claimed.
## Welfare
Request VR only on activation, use floor space with a local-space height fallback, limit
walking radius and use 30-degree snap turns with cooldown. Keep Exit VR reachable.
## Undo
Revert this slice; the flat room and exported private archives remain available. Session
end restores the default framebuffer and flat frame loop. Nell remains a redirect.
## Done when
Desktop, phone bottom-sheet, gamepad focus-ring + drawn keyboard, Quest Browser 2D,
and Meta Quest 3 immersive-vr (stereo room, 3D bubble/dock/drawer/keyboard, controller rays,
and thumbstick walk/snap-turn) pass native and headless tests and dhilipsiva confirms
/lucy/room/ loads and works on his Meta Quest 3.
