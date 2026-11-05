#[path = "src/log_snapshot/shape.rs"]
mod shape;

fn main() {
    let fingerprint = shape::fingerprint::<jojobot_domain::log::projection::State>();
    let out =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo's output directory"));
    std::fs::write(
        out.join("snapshot_shape.rs"),
        format!("pub const PROJECTION_FINGERPRINT: u32 = {fingerprint};\n"),
    )
    .expect("the generated snapshot fingerprint is writable");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/log_snapshot/shape.rs");
}
