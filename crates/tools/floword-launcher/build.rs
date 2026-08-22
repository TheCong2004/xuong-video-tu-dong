use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=payload.zip");
    println!("cargo:rerun-if-env-changed=FLOWORD_PAYLOAD_ZIP");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest_path = out_dir.join("payload.zip");

    if let Ok(custom_path) = env::var("FLOWORD_PAYLOAD_ZIP") {
        if let Ok(bytes) = fs::read(&custom_path) {
            fs::write(&dest_path, bytes).unwrap();
            return;
        }
    }

    if let Ok(bytes) = fs::read("payload.zip") {
        fs::write(&dest_path, bytes).unwrap();
    } else {
        // Create an empty dummy zip so initial compilation passes
        let dummy = [0u8; 0];
        fs::write(&dest_path, dummy).unwrap();
    }
}
