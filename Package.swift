// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Cooked",
    platforms: [.macOS(.v26)],
    targets: [
        .executableTarget(name: "Cooked"),
        .testTarget(name: "CookedTests", dependencies: ["Cooked"], exclude: ["Fixtures"]), // read via #filePath
    ]
)
