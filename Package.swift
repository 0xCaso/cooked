// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "UsageIsland",
    platforms: [.macOS(.v26)],
    targets: [
        .executableTarget(name: "UsageIsland"),
        .testTarget(name: "UsageIslandTests", dependencies: ["UsageIsland"]),
    ]
)
