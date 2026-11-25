// SPDX-License-Identifier: GPL-3.0-only

fn main() {
    println!("cargo:rerun-if-changed=src/native.c");
    pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("libde265")
        .expect("Install the libde265 development package for HEIF/HEIC decoding");
    let mut build = cc::Build::new();
    build.file("src/native.c");

    let libraries = [
        ("libavif", "1.0"),
        ("libopenjp2", "2.5"),
        ("libraw_r", "0.20"),
    ];
    for (name, version) in libraries {
        let library = pkg_config::Config::new()
            .atleast_version(version)
            .cargo_metadata(false)
            .probe(name)
            .unwrap_or_else(|error| {
                panic!("Install the {name} development package ({version}+): {error}")
            });
        build.includes(library.include_paths);
    }

    build.compile("viewer_codecs");
    for (name, _) in libraries {
        pkg_config::probe_library(name).expect("codec already detected");
    }
}
