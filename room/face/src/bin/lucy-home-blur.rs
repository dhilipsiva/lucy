//! `lucy-home-blur IN.json OUT.json`: blur the faces in a raw capture of a home, on this
//! desktop, so `lucy home import` will take it (roadmap/N8j.v2.md). Prints what it did as JSON.
//! OUT is written only when every picture was read and checked.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: lucy-home-blur IN.json OUT.json");
        std::process::exit(2);
    };
    let result = (|| -> Result<String, String> {
        if std::path::Path::new(output).exists() {
            return Err(format!("{output} already exists; I won't overwrite it"));
        }
        let text = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
        let done = lucy_face::capture::blur_capture(&text)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(output).map_err(|e| format!("{output}: {e}"))?;
        file.write_all(done.json.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("{output}: {e}"))?;
        Ok(format!(
            "{{\"blurred\":true,\"was_raw\":{},\"pictures\":{},\"faces\":{},\"output\":{:?}}}",
            done.was_raw, done.pictures, done.faces, output
        ))
    })();
    match result {
        Ok(report) => println!("{report}"),
        Err(e) => {
            eprintln!("lucy-home-blur: {e}");
            std::process::exit(1);
        }
    }
}
