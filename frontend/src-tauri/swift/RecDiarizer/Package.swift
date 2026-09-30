// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "RecDiarizer",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "RecDiarizer", type: .static, targets: ["RecDiarizer"])
    ],
    dependencies: [
        // Vendored FluidAudio 0.14.8 with a deterministic k-means patch (see vendor/FluidAudio/RECMEETILY_PATCHES.md).
        .package(path: "../../../../vendor/FluidAudio")
    ],
    targets: [
        .target(
            name: "RecDiarizer",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")]
        )
    ]
)
