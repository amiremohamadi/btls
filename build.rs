use std::process::Command;

fn main() {
    assert!(
       Command::new("/usr/bin/env")
            .arg("python")
            .arg("./scripts/generate.py")
            .output()
            .unwrap()
            .status
            .success()
    );
    println!("cargo:rerun-if-changed=./target/builtins.gen.rs");
}
