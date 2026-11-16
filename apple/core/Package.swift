// swift-tools-version: 6.2
// The Rust core for the Apple apps and what they share (docs/apple/plan.md §1):
//   FobFFI   the XCFramework apple/core/build-core.sh builds (not committed)
//   FobCore  the generated Swift bindings (Sources/FobCore/Generated, not
//            committed), the app model and its scene loader
//   FobUI    the theme (Theme.kt's colour roles), Manrope, shared views
// Run build-core.sh (--sim, --mac or --release) before building.

import PackageDescription

let package = Package(
    name: "FobCore",
    platforms: [.iOS("26.7"), .macOS("26.0")],
    products: [
        .library(name: "FobCore", targets: ["FobCore"]),
        .library(name: "FobUI", targets: ["FobUI"]),
    ],
    targets: [
        .binaryTarget(name: "FobFFI", path: "FobFFI.xcframework"),
        .target(
            name: "FobCore",
            dependencies: ["FobFFI"],
            path: "Sources/FobCore",
            linkerSettings: [
                // What the core needs from the system: rustls' platform
                // verifier checks certificates with Security, reqwest reads
                // proxy settings through SystemConfiguration, and SQLCipher
                // asks Security and CoreFoundation for random bytes.
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Foundation"),
            ]
        ),
        .target(
            name: "FobUI",
            dependencies: ["FobCore"],
            path: "Sources/FobUI",
            resources: [.copy("Resources/Manrope.ttf")]
        ),
    ]
)
