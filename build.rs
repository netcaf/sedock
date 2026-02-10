fn main() {
    // 确保在 Linux 上编译
    println!("cargo:rerun-if-changed=build.rs");
    
    #[cfg(not(target_os = "linux"))]
    compile_error!("This tool only works on Linux");
}