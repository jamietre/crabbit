// crates/server/build.rs
fn main() {
    println!("cargo:rerun-if-changed=../../web/build");
    println!("cargo:rerun-if-changed=../../web/src");
}
