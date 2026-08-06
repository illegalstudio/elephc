// Lot 2 of IOS_TARGET_SPEC.md: a native SwiftUI host driven entirely by
// compiled PHP, on macOS and on iOS.
//
// This file contains no application logic. It asks the elephc-compiled library
// for a view tree, turns that tree into real SwiftUI views, and sends button
// actions back. Layout, labels, pluralisation and state all live on the PHP
// side; swapping view.php changes the app without touching a line of Swift.
//
// The library is linked statically, so the exports are ordinary C symbols
// declared in elephc_abi.h. That is the delivery form an Xcode project
// consumes, and unlike dlopen it works unchanged on iOS.

import SwiftUI

// MARK: - The C ABI elephc exposes

/// Calls into the statically linked library.
///
/// Every string an export returns is owned by *this* side and released through
/// `elephc_free`, which is why each call copies into a Swift `String` and frees
/// immediately rather than holding the pointer.
enum Elephc {
    /// Prepares heap and globals. Safe to call more than once.
    static func start() -> Bool {
        elephc_init() == 0
    }

    /// Copies an elephc-owned buffer into a Swift `String` and releases it.
    ///
    /// The buffer is a PHP byte string, so the length is authoritative — it is
    /// not NUL-terminated and may legitimately contain interior zero bytes.
    private static func take(_ result: ElephcStr) -> String {
        guard let ptr = result.ptr else { return "" }
        let bytes = UnsafeRawPointer(ptr).assumingMemoryBound(to: UInt8.self)
        let text = String(decoding: UnsafeBufferPointer(start: bytes, count: result.len), as: UTF8.self)
        elephc_free(UnsafeMutableRawPointer(mutating: ptr))
        return text
    }

    static func render() -> String { take(render_view()) }

    static func dispatch(_ action: String) -> String {
        let utf8 = Array(action.utf8).map { CChar(bitPattern: $0) }
        return utf8.withUnsafeBufferPointer { buffer in
            take(elephc_dispatch(buffer.baseAddress, action.utf8.count))
        }
    }
}

/// `dispatch` collides with Swift's Dispatch module at the call site, so the C
/// symbol is reached through a renamed shim.
@_silgen_name("dispatch")
func elephc_dispatch(_ action: UnsafePointer<CChar>?, _ length: Int) -> ElephcStr

// MARK: - The view protocol

/// One node of the tree PHP emits. The host understands these node types and
/// nothing else; adding a widget means teaching both sides one new `t` value.
struct Node: Decodable {
    let t: String
    let v: String?
    let style: String?
    let label: String?
    let action: String?
    let children: [Node]?
}

// MARK: - Rendering

struct ContentView: View {
    @State private var tree: Node?
    @State private var decodeError: String?

    var body: some View {
        VStack {
            if let error = decodeError {
                Text(error).foregroundStyle(.red).font(.callout)
            } else if let tree {
                render(tree)
            } else {
                ProgressView()
            }
        }
        .padding(28)
        .onAppear { load { Elephc.render() } }
    }

    private func load(_ produce: () -> String) {
        let json = produce()
        do {
            tree = try JSONDecoder().decode(Node.self, from: Data(json.utf8))
            decodeError = nil
        } catch {
            decodeError = "the view tree did not decode: \(error)"
        }
    }

    /// Turns a node into SwiftUI. Type-erased because the function recurses:
    /// a `some View` return type cannot describe a shape that depends on data.
    private func render(_ node: Node) -> AnyView {
        switch node.t {
        case "vstack":
            return AnyView(VStack(spacing: 14) { children(of: node) })
        case "hstack":
            return AnyView(HStack(spacing: 10) { children(of: node) })
        case "text":
            return AnyView(Text(node.v ?? "").font(font(for: node.style)))
        case "button":
            return AnyView(Button(node.label ?? "") {
                let action = node.action ?? ""
                load { Elephc.dispatch(action) }
            }
            .buttonStyle(.bordered))
        default:
            return AnyView(Text("unknown node: \(node.t)").foregroundStyle(.secondary))
        }
    }

    @ViewBuilder
    private func children(of node: Node) -> some View {
        ForEach(Array((node.children ?? []).enumerated()), id: \.offset) { _, child in
            render(child)
        }
    }

    private func font(for style: String?) -> Font {
        switch style {
        case "title": return .title2.bold()
        case "caption": return .caption
        default: return .body
        }
    }
}

// MARK: - Entry point

/// Headless check of the whole round trip: render, decode, dispatch, observe
/// the state PHP kept between calls.
///
/// Exists so the example is verifiable without a display — a GUI that merely
/// launches proves nothing about whether the tree decoded or the state moved.
enum SelfTest {
    static func run() -> Never {
        guard Elephc.start() else { print("FAIL: elephc_init"); exit(2) }

        func tree() -> Node? {
            try? JSONDecoder().decode(Node.self, from: Data(Elephc.render().utf8))
        }
        func body(_ node: Node?) -> String {
            node?.children?.first(where: { $0.style == "body" })?.v ?? "<missing>"
        }

        guard let initial = tree(), initial.t == "vstack", initial.children?.count == 4 else {
            print("FAIL: unexpected root shape"); exit(1)
        }
        let start = body(initial)

        _ = Elephc.dispatch("inc")
        _ = Elephc.dispatch("inc")
        let twice = body(tree())

        _ = Elephc.dispatch("dec")
        let once = body(tree())

        _ = Elephc.dispatch("reset")
        let cleared = body(tree())

        print("initial=\(start) after++=\(twice) after-=\(once) reset=\(cleared)")
        let expected = start == "nothing yet"
            && twice == "2 items"
            && once == "one item"
            && cleared == "nothing yet"
        if !expected { print("FAIL: state did not move as the PHP side defines it"); exit(1) }
        print("PASS: the view tree, the string ABI and PHP-side state all round-trip")
        exit(0)
    }
}

@main
enum Entry {
    static func main() {
        if CommandLine.arguments.contains("--selftest") {
            SelfTest.run()
        }
        _ = Elephc.start()
        ViewProtocolApp.main()
    }
}

struct ViewProtocolApp: App {
    var body: some Scene {
        WindowGroup("elephc → SwiftUI") {
            ContentView()
        }
    }
}
