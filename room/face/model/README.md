# The face detector's model

`version-RFB-320.onnx` is UltraFace RFB-320 from
[Linzaer/Ultra-Light-Fast-Generic-Face-Detector-1MB](https://github.com/Linzaer/Ultra-Light-Fast-Generic-Face-Detector-1MB)
(MIT licence), as published in the ONNX model zoo
(<https://huggingface.co/onnxmodelzoo/version-RFB-320>). 1,270,727 bytes, sha256
`34cd7e60aeff28744c657de7a3dc64e872d506741de66987f3426f2b79f88017`; `src/lib.rs` pins that
digest and a test checks it. Input 1×3×240×320 RGB, `(x − 127) / 128`; outputs scores and
boxes already decoded to corners in 0..1.

`../tests/portrait.jpg` is Alexander Gardner's 1863 photograph of Abraham Lincoln (O-77),
public domain, from Wikimedia Commons: the fixture a face must be found in.

`version-RFB-640.onnx` is the same family at 640×480, from
<https://huggingface.co/onnxmodelzoo/version-RFB-640> (MIT): 1,588,012 bytes, sha256
`8f4c659275977e7a3bfbfa339a9c769ad793df50f9c0baa8c14b11baa1646430`. Only the desktop tool
(`--features desktop`, `lucy-home-blur`) carries it; the browser's worker does not.
