// swift-tools-version:5.5
import PackageDescription

let package = Package(
  name: "tauri-plugin-workchat",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-workchat",
      type: .static,
      targets: ["tauri-plugin-workchat"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-workchat",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
