// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "LascoPhotoImportKit",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "LascoPhotoImportKit", type: .dynamic, targets: ["LascoPhotoImportKit"]),
    ],
    targets: [
        .target(name: "LascoPhotoImportKit"),
        .testTarget(name: "LascoPhotoImportKitTests", dependencies: ["LascoPhotoImportKit"]),
    ]
)
