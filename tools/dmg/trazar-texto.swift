// Prints the outline of a line of text as the `d` attribute of an SVG path, so assets/dmg/fondo.svg
// holds no live text (fonts differ between Macs and rsvg-convert would draw something else).
//
// Usage: swiftc -O -o trazar-texto tools/dmg/trazar-texto.swift
//        ./trazar-texto "Drag Altrune to Applications" 13 0.4 PATH/Ubuntu-Light.ttf
//   text, size in SVG units, extra spacing between letters in SVG units, and the font file.
//   The path starts at x = 0 on the baseline y = 0; the width goes to standard error so the
//   caller can centre the text.
//
// The font is always a file given by path, never the system font of the Mac that runs it: Apple's
// San Francisco must not end up as artwork in the repository (see THIRD_PARTY_NOTICES.md). The
// committed sentence was traced from Ubuntu-Light.ttf, the font egui already brings inside the
// app (crate epaint_default_fonts 0.37.0, fonts/Ubuntu-Light.ttf, Ubuntu Font Licence 1.0). It is
// only needed when the sentence changes: the committed SVG already has the path.
import AppKit
import CoreText

let a = CommandLine.arguments
guard a.count == 5, let size = Double(a[2]), let spacing = Double(a[3]) else {
    FileHandle.standardError.write(Data("usage: trazar-texto TEXT SIZE SPACING FONTFILE\n".utf8))
    exit(2)
}
guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(URL(fileURLWithPath: a[4]) as CFURL) as? [CTFontDescriptor],
      let descriptor = descriptors.first else {
    FileHandle.standardError.write(Data("trazar-texto: cannot read a font from \(a[4])\n".utf8))
    exit(2)
}
let font = CTFontCreateWithFontDescriptor(descriptor, CGFloat(size), nil)
let attributed = NSAttributedString(string: a[1], attributes: [.font: font, .kern: spacing])
let line = CTLineCreateWithAttributedString(attributed)
func f(_ v: CGFloat) -> String {
    let s = String(format: "%.2f", Double(v))
    let t = s.contains(".") ? String(s.reversed().drop(while: { $0 == "0" }).drop(while: { $0 == "." }).reversed()) : s
    return t == "-0" ? "0" : t
}
var d = ""
let runs = CTLineGetGlyphRuns(line) as! [CTRun]
for run in runs {
    let count = CTRunGetGlyphCount(run)
    var glyphs = [CGGlyph](repeating: 0, count: count)
    var positions = [CGPoint](repeating: .zero, count: count)
    CTRunGetGlyphs(run, CFRange(location: 0, length: 0), &glyphs)
    CTRunGetPositions(run, CFRange(location: 0, length: 0), &positions)
    let runFont = (CTRunGetAttributes(run) as! [CFString: Any])[kCTFontAttributeName] as! CTFont
    for i in 0..<count {
        guard let path = CTFontCreatePathForGlyph(runFont, glyphs[i], nil) else { continue }
        let origin = positions[i]
        path.applyWithBlock { e in
            let el = e.pointee
            func p(_ k: Int) -> String { "\(f(el.points[k].x + origin.x)) \(f(-(el.points[k].y + origin.y)))" }
            switch el.type {
            case .moveToPoint: d += "M\(p(0))"
            case .addLineToPoint: d += "L\(p(0))"
            case .addQuadCurveToPoint: d += "Q\(p(0)) \(p(1))"
            case .addCurveToPoint: d += "C\(p(0)) \(p(1)) \(p(2))"
            case .closeSubpath: d += "Z"
            @unknown default: break
            }
        }
    }
}
print(d)
let width = CTLineGetTypographicBounds(line, nil, nil, nil)
FileHandle.standardError.write(Data("width \(f(CGFloat(width)))\n".utf8))
