fn main() {
    println!("cargo:rerun-if-changed=world-src/");

    let mut build = cc::Build::new();

    build
        .cpp(true)
        .std("c++11")
        .include("world-src")
        .warnings(false);

    let sources = [
        "world-src/cheaptrick.cpp",
        "world-src/codec.cpp",
        "world-src/common.cpp",
        "world-src/d4c.cpp",
        "world-src/dio.cpp",
        "world-src/fft.cpp",
        "world-src/harvest.cpp",
        "world-src/matlabfunctions.cpp",
        "world-src/stonemask.cpp",
        "world-src/synthesis.cpp",
        "world-src/synthesisrealtime.cpp",
    ];

    for source in &sources {
        build.file(source);
    }

    build.compile("world");
}
