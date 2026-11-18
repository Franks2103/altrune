// Prints "id x y width height" of the first on-screen window of one app with one title, for
// example: ventana Finder Altrune. Exits 1 when there is none. It only reads the window list
// (CGWindowListCopyWindowInfo); it never changes a window, a setting or an app.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3 else {
    FileHandle.standardError.write(Data("usage: ventana OWNER TITLE\n".utf8))
    exit(2)
}
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    guard (w[kCGWindowOwnerName as String] as? String) == args[1],
          (w[kCGWindowName as String] as? String) == args[2],
          (w[kCGWindowLayer as String] as? Int) == 0,
          let id = w[kCGWindowNumber as String] as? Int,
          let b = w[kCGWindowBounds as String] as? [String: Any] else { continue }
    func n(_ k: String) -> Int { Int((b[k] as? Double) ?? Double((b[k] as? Int) ?? 0)) }
    print(id, n("X"), n("Y"), n("Width"), n("Height"))
    exit(0)
}
exit(1)
