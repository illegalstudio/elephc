// Shows the elephc sandbox probe's report on screen, and prints it so a
// `devicectl … --console` run captures the same text.
//
// The report is produced entirely by compiled PHP; this file only chooses the
// directory to probe and renders the result in a monospaced scroll view.

import SwiftUI

/// Runs the probe against the app's own temporary directory.
///
/// `NSTemporaryDirectory()` is inside the app container on a device and inside a
/// macOS-hosted path in the Simulator — the difference between those two is a
/// large part of what this probe exists to reveal.
func runProbe() -> String {
    guard elephc_init() == 0 else { return "elephc_init failed" }

    let dir = NSTemporaryDirectory()
    let utf8 = Array(dir.utf8).map { CChar(bitPattern: $0) }
    let result = utf8.withUnsafeBufferPointer { buffer in
        probe(buffer.baseAddress, dir.utf8.count)
    }
    guard let ptr = result.ptr else { return "probe returned no report" }
    let bytes = UnsafeRawPointer(ptr).assumingMemoryBound(to: UInt8.self)
    let text = String(decoding: UnsafeBufferPointer(start: bytes, count: result.len), as: UTF8.self)
    elephc_free(UnsafeMutableRawPointer(mutating: ptr))
    return text
}

struct ProbeView: View {
    @State private var report = "running…"

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            Text(report)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        }
        .onAppear {
            let text = runProbe()
            report = text
            // Also to stdout, so `devicectl device process launch --console`
            // yields the same report without reading it off the screen.
            print(text)
        }
    }
}

@main
enum Entry {
    static func main() {
        if CommandLine.arguments.contains("--stdout") {
            print(runProbe())
            exit(0)
        }
        ProbeApp.main()
    }
}

struct ProbeApp: App {
    var body: some Scene {
        WindowGroup("elephc sandbox probe") {
            ProbeView()
        }
    }
}
