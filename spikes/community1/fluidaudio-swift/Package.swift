// swift-tools-version:5.10
import PackageDescription

let package = Package(
    name: "fluidaudio-swift",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.14.1"),
    ],
    targets: [
        .executableTarget(
            name: "fluidaudio-swift",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")]
        ),
    ]
)
