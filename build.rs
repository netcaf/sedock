fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(not(target_os = "linux"))]
    compile_error!("This tool only works on Linux");

    // Capture build time at compile time
    let output = std::process::Command::new("date")
        .args(&["-u", "+%Y-%m-%d %H:%M:%S UTC"])
        .output()
        .expect("failed to get build time");
    let build_time = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}
