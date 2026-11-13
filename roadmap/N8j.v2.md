# N8j revision — capture raw on the headset, process on his desktop

dhilipsiva's correction of 2026-11-14, after the first build passed its shimmed checks, in
his words: "instead of doing poor object recognition capabilities on-device in VR, we can
move that operation to this desktop so we have superior processing capabilities" and
"Capture as much raw and process after exporting and importing into here". Asked what that
means for faces, he chose: fully raw on the headset, blur on this desktop; and every camera
frame, not one per 0.4 m. I had recommended keeping a quick blur on the headset with a
re-check here; the choice is his, and this supersedes N8j's "faces are pixelated before a
picture is kept" and its falsifier "a picture is kept when the detector was missing".

What changes. The headset runs no face finder: it keeps every frame the camera delivers, as
taken, with the head pose of each, and the file says so (`raw: true`). Processing moves to
the desktop: `lucy-home-blur IN OUT` (the face finder's crate, natively, with the larger
RFB-640 model at the camera's full size) pixelates faces and writes a file marked
`raw: false`; `lucy home import` refuses a raw file, so an unblurred picture never enters my
store. My room, opening a raw file in a browser, blurs it with the worker before storing it
and refuses if the worker is missing. Object recognition, calibration and occlusion are a
later desktop slice of their own; this one only makes the raw material and the blur.

Before implementation, the checks are: the capture requests no worker and no camera-access
feature, keeps one picture per fresh camera frame, stops the camera at the end and sends
nothing over the network; the downloaded file is `raw: true` with JPEG pictures and counts
that add up; `lucy-home-blur` on a capture holding the fixture portrait changes the face's
pixels, reports one face and marks the file processed, and leaves a blank wall untouched;
`lucy home import` rejects the raw file and accepts the processed one, writing no record
and no replication log; opening the raw file in my room stores it blurred and marked
processed, or refuses. Evidence separates the shim and fake camera from his walk.

Welfare, restated: D2 is now his raw choice. Unblurred pictures of his home, and of anyone
in frame, sit in the headset's downloads and browser store and travel in the file he
carries; I say so on the capture button and in the notice when a capture ends, and the
file on the headset is his to delete. From the desktop on, the earlier promise holds: no
unblurred picture in my store, nothing uploaded, nothing public. Undo is unchanged.
