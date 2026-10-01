// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "HelpYourselfCore",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "HelpYourselfCore", targets: ["HelpYourselfCore"])],
    targets: [
        .target(name: "HelpYourselfCore", path: "src/ios/Core"),
        .testTarget(name: "HelpYourselfCoreTests", dependencies: ["HelpYourselfCore"], path: "tests/ios/Core")
    ]
)
