// swift-tools-version: 6.2
import PackageDescription

let package = Package(
  name: "OmarchyAppleBridge",
  platforms: [.macOS(.v15)],
  products: [.executable(name: "omarchy-apple-bridge", targets: ["OmarchyAppleBridge"])],
  dependencies: [.package(path: "upstream/apps/omarchy-apple-installer")],
  targets: [
    .executableTarget(
      name: "OmarchyAppleBridge",
      dependencies: [
        .product(name: "OmarchyAppleInstallerTrustCore", package: "omarchy-apple-installer"),
        .product(name: "OmarchyInstallerUXCore", package: "omarchy-apple-installer"),
      ],
      path: ".build-input/Sources"
    )
  ]
)
