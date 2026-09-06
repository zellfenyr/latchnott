fn main() {
    println!("cargo:rerun-if-changed=ui/main.slint");
    println!("cargo:rerun-if-changed=assets/tray.svg");

    slint_build::compile("ui/main.slint").expect("failed to compile ui/main.slint");
}
