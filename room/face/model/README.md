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
