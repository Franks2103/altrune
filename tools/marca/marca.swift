// Altrune brand asset generator.
//
// Rebuilds every file in assets/branding/ from the official logo (the master
// reference), without redrawing anything:
//   extraer  - colour-to-alpha extraction with real transparency, the layers,
//              the interface sizes and the fidelity proof
//   icono    - the macOS app icon (AppIcon-1024.png and the iconset PNGs)
//   paleta   - measures the palette and prints the WCAG contrast table
//
// Usage (see generar.sh, which runs all of it):
//   swift marca.swift extraer <logo.png> <out-dir>
//   swift marca.swift icono   <out-dir> <iconset-dir>
//   swift marca.swift paleta  <logo.png>
//
// Only Apple frameworks (ImageIO, CoreGraphics); no third-party code.

import Foundation
import ImageIO
import CoreGraphics
import UniformTypeIdentifiers

// MARK: - Raster type

/// Straight (non-premultiplied) RGBA. r, g, b in 0...255, a in 0...1.
struct Img {
    var w: Int
    var h: Int
    var d: [Float]
    init(w: Int, h: Int) { self.w = w; self.h = h; d = [Float](repeating: 0, count: w * h * 4) }
}

let srgb = CGColorSpace(name: CGColorSpace.sRGB)!

func die(_ msg: String) -> Never {
    FileHandle.standardError.write((msg + "\n").data(using: .utf8)!)
    exit(1)
}

/// Decodes a PNG into 8-bit sRGB premultiplied RGBA bytes.
func decode8(_ path: String) -> (w: Int, h: Int, bytes: [UInt8]) {
    guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
          let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else { die("cannot read \(path)") }
    let w = img.width, h = img.height
    var buf = [UInt8](repeating: 0, count: w * h * 4)
    let ctx = CGContext(data: &buf, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                        space: srgb, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .none
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
    return (w, h, buf)
}

/// Writes straight-alpha RGBA as an 8-bit PNG. Colours of fully transparent
/// pixels are stored as zero so the output does not depend on junk values.
func writePNG(_ img: Img, to path: String) {
    var bytes = [UInt8](repeating: 0, count: img.w * img.h * 4)
    for i in 0..<(img.w * img.h) {
        let a = img.d[i * 4 + 3]
        let aq = UInt8(max(0, min(255, (a * 255).rounded())))
        bytes[i * 4 + 3] = aq
        if aq == 0 { continue }
        for c in 0..<3 { bytes[i * 4 + c] = UInt8(max(0, min(255, img.d[i * 4 + c].rounded()))) }
    }
    writeBytesPNG(bytes, w: img.w, h: img.h, alpha: .last, to: path)
}

func writeBytesPNG(_ bytes: [UInt8], w: Int, h: Int, alpha: CGImageAlphaInfo, to path: String) {
    let data = Data(bytes)
    let provider = CGDataProvider(data: data as CFData)!
    let cg = CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                     space: srgb, bitmapInfo: CGBitmapInfo(rawValue: alpha.rawValue),
                     provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
    let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: path) as CFURL,
                                               UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, cg, nil)
    if !CGImageDestinationFinalize(dest) { die("cannot write \(path)") }
}

// MARK: - Resampling (Lanczos-3 on premultiplied floats)

func lanczos(_ x: Double) -> Double {
    let ax = abs(x)
    if ax < 1e-9 { return 1 }
    if ax >= 3 { return 0 }
    let px = Double.pi * ax
    return 3 * sin(px) * sin(px / 3) / (px * px)
}

/// Resizes with a Lanczos-3 kernel stretched by the scale factor when
/// shrinking (so it also acts as a proper low-pass), in premultiplied space.
func resize(_ src: Img, w w2: Int, h h2: Int) -> Img {
    // premultiply
    var p = [Float](repeating: 0, count: src.w * src.h * 4)
    for i in 0..<(src.w * src.h) {
        let a = src.d[i * 4 + 3]
        p[i * 4] = src.d[i * 4] * a; p[i * 4 + 1] = src.d[i * 4 + 1] * a
        p[i * 4 + 2] = src.d[i * 4 + 2] * a; p[i * 4 + 3] = a
    }
    func weights(_ n1: Int, _ n2: Int) -> [(start: Int, w: [Float])] {
        let s = Double(n1) / Double(n2)
        let sc = max(1.0, s)
        var out = [(Int, [Float])]()
        for o in 0..<n2 {
            let c = (Double(o) + 0.5) * s - 0.5
            let lo = Int(floor(c - 3 * sc)), hi = Int(ceil(c + 3 * sc))
            var ws = [Double](); var tot = 0.0
            var start = lo
            for i in lo...hi {
                let wv = (i < 0 || i >= n1) ? 0.0 : lanczos((Double(i) - c) / sc)
                ws.append(wv); tot += wv
            }
            // trim to the valid range
            while start < 0 { ws.removeFirst(); start += 1 }
            while start + ws.count > n1 { ws.removeLast() }
            out.append((start, ws.map { Float($0 / tot) }))
        }
        return out
    }
    let wx = weights(src.w, w2), wy = weights(src.h, h2)
    var tmp = [Float](repeating: 0, count: w2 * src.h * 4)
    for y in 0..<src.h {
        for x in 0..<w2 {
            let (st, ws) = wx[x]
            var a0: Float = 0, a1: Float = 0, a2: Float = 0, a3: Float = 0
            for (k, wv) in ws.enumerated() {
                let i = (y * src.w + st + k) * 4
                a0 += wv * p[i]; a1 += wv * p[i + 1]; a2 += wv * p[i + 2]; a3 += wv * p[i + 3]
            }
            let o = (y * w2 + x) * 4
            tmp[o] = a0; tmp[o + 1] = a1; tmp[o + 2] = a2; tmp[o + 3] = a3
        }
    }
    var out = Img(w: w2, h: h2)
    for y in 0..<h2 {
        let (st, ws) = wy[y]
        for x in 0..<w2 {
            var a0: Float = 0, a1: Float = 0, a2: Float = 0, a3: Float = 0
            for (k, wv) in ws.enumerated() {
                let i = ((st + k) * w2 + x) * 4
                a0 += wv * tmp[i]; a1 += wv * tmp[i + 1]; a2 += wv * tmp[i + 2]; a3 += wv * tmp[i + 3]
            }
            let a = max(0, min(1, a3))
            let o = (y * w2 + x) * 4
            out.d[o + 3] = a
            if a > 1e-4 {
                for (c, v) in [a0, a1, a2].enumerated() { out.d[o + c] = max(0, min(255, v / a)) }
            }
        }
    }
    return out
}

// MARK: - Extraction

struct Extraction {
    var bg: [Double]          // measured background colour
    var whole: Img            // full canvas, same size as the original
    var splitRow: Int         // first row of the wordmark
}

func measureBackground(_ bytes: [UInt8], w: Int, h: Int) -> [Double] {
    var s = [Double](repeating: 0, count: 3); var n = 0.0
    for y in 0..<h { for x in 0..<w where x < 250 || x >= w - 250 || y < 250 || y >= h - 250 {
        let i = (y * w + x) * 4
        for c in 0..<3 { s[c] += Double(bytes[i + c]) }
        n += 1
    } }
    return s.map { $0 / n }
}

/// Colour-to-alpha un-blending against the measured background.
///
/// Plain "max over channels" colour-to-alpha would make a light-blue interior
/// semi-transparent. Instead, interior pixels (well away from the background
/// everywhere in a 5x5 window) are exact foreground: alpha 1, colour as in the
/// file. Edge pixels are p = a*F + (1-a)*B, where the local foreground F is
/// propagated ring by ring from the nearest interior pixels. alpha is how far p
/// lies from B along F-B, and the colour is (p - (1-a)*B)/a.
func extract(_ path: String) -> Extraction {
    let (w, h, px) = decode8(path)
    let B = measureBackground(px, w: w, h: h)
    let n = w * h
    var dist = [Float](repeating: 0, count: n)
    for i in 0..<n {
        var m: Double = 0
        for c in 0..<3 { m = max(m, abs(Double(px[i * 4 + c]) - B[c])) }
        dist[i] = Float(m)
    }
    // solid: every pixel of the 5x5 window is far from the background
    var far = [Bool](repeating: false, count: n)
    for i in 0..<n { far[i] = dist[i] > 30 }
    // separable erosion by radius 2
    var er1 = [Bool](repeating: false, count: n)
    for y in 0..<h { for x in 0..<w {
        var ok = true
        for dx in -2...2 { let xx = x + dx; if xx < 0 || xx >= w || !far[y * w + xx] { ok = false; break } }
        er1[y * w + x] = ok
    } }
    var solid = [Bool](repeating: false, count: n)
    for y in 0..<h { for x in 0..<w {
        var ok = true
        for dy in -2...2 { let yy = y + dy; if yy < 0 || yy >= h || !er1[yy * w + x] { ok = false; break } }
        solid[y * w + x] = ok
    } }
    // propagate the foreground colour outward ring by ring
    var F = [Float](repeating: 0, count: n * 3)
    var known = solid
    for i in 0..<n where solid[i] { for c in 0..<3 { F[i * 3 + c] = Float(px[i * 4 + c]) } }
    for _ in 0..<8 {
        var newKnown = known
        var newF = F
        for y in 0..<h { for x in 0..<w {
            let i = y * w + x
            if known[i] || dist[i] <= 2 { continue }
            var cnt: Float = 0; var s: [Float] = [0, 0, 0]
            for dy in -1...1 { for dx in -1...1 {
                let xx = x + dx, yy = y + dy
                if xx < 0 || yy < 0 || xx >= w || yy >= h { continue }
                let j = yy * w + xx
                if known[j] { cnt += 1; for c in 0..<3 { s[c] += F[j * 3 + c] } }
            } }
            if cnt > 0 { newKnown[i] = true; for c in 0..<3 { newF[i * 3 + c] = s[c] / cnt } }
        } }
        known = newKnown; F = newF
    }
    var alpha = [Float](repeating: 0, count: n)
    for i in 0..<n {
        if solid[i] { alpha[i] = 1; continue }
        if !known[i] { continue }
        var vf: Float = 0, ff: Float = 0
        for c in 0..<3 {
            let f = F[i * 3 + c] - Float(B[c]); let v = Float(px[i * 4 + c]) - Float(B[c])
            vf += v * f; ff += f * f
        }
        alpha[i] = ff > 1 ? max(0, min(1, vf / ff)) : 0
        if alpha[i] < 0.04 { alpha[i] = 0 }          // background noise and ringing
        if alpha[i] > 0.985 { alpha[i] = 1 }
    }
    // drop faint alpha that is not attached to a real edge (JPEG-style ringing)
    var cleaned = alpha
    for y in 0..<h { for x in 0..<w {
        let i = y * w + x
        if alpha[i] > 0 && alpha[i] < 0.10 {
            var attached = false
            for dy in -2...2 { for dx in -2...2 {
                let xx = x + dx, yy = y + dy
                if xx < 0 || yy < 0 || xx >= w || yy >= h { continue }
                if alpha[yy * w + xx] >= 0.30 { attached = true }
            } }
            if !attached { cleaned[i] = 0 }
        }
    } }
    alpha = cleaned
    var out = Img(w: w, h: h)
    for i in 0..<n {
        let a = Float((alpha[i] * 255).rounded() / 255)
        out.d[i * 4 + 3] = a
        if a <= 0 { continue }
        for c in 0..<3 {
            var v: Float
            if a >= 1 { v = Float(px[i * 4 + c]) }
            else if a >= 0.25 { v = (Float(px[i * 4 + c]) - (1 - a) * Float(B[c])) / a }
            else { v = F[i * 3 + c] }
            out.d[i * 4 + c] = max(0, min(255, v))
        }
    }
    // split row: middle of the widest run of empty rows between symbol and wordmark
    var rowHas = [Bool](repeating: false, count: h)
    for y in 0..<h { for x in 0..<w where out.d[(y * w + x) * 4 + 3] > 0 { rowHas[y] = true; break } }
    var best = (0, 0, 0)   // length, start, end
    var y = 0
    var first = -1, last = -1
    for yy in 0..<h where rowHas[yy] { if first < 0 { first = yy }; last = yy }
    while y <= last {
        if y > first && !rowHas[y] {
            var e = y; while e <= last && !rowHas[e] { e += 1 }
            if e - y > best.0 { best = (e - y, y, e - 1) }
            y = e
        } else { y += 1 }
    }
    let split = (best.1 + best.2) / 2 + 1
    return Extraction(bg: B, whole: out, splitRow: split)
}

func bbox(_ img: Img) -> (x0: Int, y0: Int, x1: Int, y1: Int) {
    var x0 = img.w, y0 = img.h, x1 = -1, y1 = -1
    for y in 0..<img.h { for x in 0..<img.w where img.d[(y * img.w + x) * 4 + 3] > 0 {
        x0 = min(x0, x); x1 = max(x1, x); y0 = min(y0, y); y1 = max(y1, y)
    } }
    return (x0, y0, x1, y1)
}

func crop(_ img: Img, x0: Int, y0: Int, w: Int, h: Int) -> Img {
    var o = Img(w: w, h: h)
    for y in 0..<h { for x in 0..<w {
        let sx = x0 + x, sy = y0 + y
        if sx < 0 || sy < 0 || sx >= img.w || sy >= img.h { continue }
        for c in 0..<4 { o.d[(y * w + x) * 4 + c] = img.d[(sy * img.w + sx) * 4 + c] }
    } }
    return o
}

func clearRows(_ img: Img, from: Int, to: Int) -> Img {
    var o = img
    for y in max(0, from)..<min(img.h, to) { for x in 0..<img.w {
        for c in 0..<4 { o.d[(y * img.w + x) * 4 + c] = 0 }
    } }
    return o
}

/// Wordmark as a white mask carrying only its alpha.
func whiteMask(_ img: Img) -> Img {
    var o = img
    for i in 0..<(img.w * img.h) {
        if o.d[i * 4 + 3] > 0 { o.d[i * 4] = 255; o.d[i * 4 + 1] = 255; o.d[i * 4 + 2] = 255 }
    }
    return o
}

// MARK: - Fidelity

struct Diff { var maxD = [Int](repeating: 0, count: 3); var sum = [Double](repeating: 0, count: 3); var n = 0; var over4 = 0 }

/// Composites premultiplied layers (decoded from disk) over the background,
/// rounds to 8 bits and compares with the original.
func compareFidelity(orig: [UInt8], ow: Int, oh: Int, layers: [(bytes: [UInt8], w: Int, h: Int, ox: Int, oy: Int)],
                     bg: [Double], onlyShape: Bool) -> Diff {
    var comp = [Double](repeating: 0, count: ow * oh * 3)
    var cov = [Bool](repeating: false, count: ow * oh)
    for i in 0..<(ow * oh) { for c in 0..<3 { comp[i * 3 + c] = bg[c] } }
    for L in layers {
        for y in 0..<L.h { for x in 0..<L.w {
            let ox = L.ox + x, oy = L.oy + y
            if ox < 0 || oy < 0 || ox >= ow || oy >= oh { continue }
            let i = (y * L.w + x) * 4
            let a = Double(L.bytes[i + 3]) / 255
            if a > 0 { cov[oy * ow + ox] = true }
            for c in 0..<3 {
                let d = oy * ow + ox
                comp[d * 3 + c] = Double(L.bytes[i + c]) + (1 - a) * comp[d * 3 + c]
            }
        } }
    }
    var diff = Diff()
    for i in 0..<(ow * oh) {
        if onlyShape && !cov[i] { continue }
        diff.n += 1
        var big = false
        for c in 0..<3 {
            let v = Int(max(0, min(255, comp[i * 3 + c].rounded())))
            let d = abs(v - Int(orig[i * 4 + c]))
            diff.maxD[c] = max(diff.maxD[c], d); diff.sum[c] += Double(d)
            if d > 4 { big = true }
        }
        if big { diff.over4 += 1 }
    }
    return diff
}

func describe(_ name: String, _ d: Diff) -> String {
    let mean = d.sum.map { String(format: "%.4f", $0 / Double(d.n)) }
    return "  \(name): max diff R/G/B = \(d.maxD[0])/\(d.maxD[1])/\(d.maxD[2]); mean diff R/G/B = \(mean[0])/\(mean[1])/\(mean[2]); pixels compared = \(d.n); pixels with any channel > 4: \(d.over4)"
}

// MARK: - Command: extraer

let margin = 16

func cmdExtraer(_ logo: String, _ out: String) {
    let e = extract(logo)
    let (ow, oh, orig) = decode8(logo)
    let whole = e.whole
    let symbolFull = clearRows(whole, from: e.splitRow, to: whole.h)
    let wordFull = clearRows(whole, from: 0, to: e.splitRow)

    func trimmed(_ img: Img, _ bb: (x0: Int, y0: Int, x1: Int, y1: Int)) -> (Img, Int, Int) {
        let x0 = bb.x0 - margin, y0 = bb.y0 - margin
        let w = bb.x1 - bb.x0 + 1 + 2 * margin, h = bb.y1 - bb.y0 + 1 + 2 * margin
        return (crop(img, x0: x0, y0: y0, w: w, h: h), x0, y0)
    }
    let bbSym = bbox(symbolFull), bbAll = bbox(whole)
    let (simbolo, sx, sy) = trimmed(symbolFull, bbSym)
    let (logotipo, lx, ly) = trimmed(whole, bbAll)
    let lsimbolo = crop(symbolFull, x0: lx, y0: ly, w: logotipo.w, h: logotipo.h)
    let lpalabra = whiteMask(crop(wordFull, x0: lx, y0: ly, w: logotipo.w, h: logotipo.h))

    func p(_ n: String) -> String { out + "/" + n }
    writePNG(simbolo, to: p("altrune-simbolo.png"))
    writePNG(logotipo, to: p("altrune-logotipo.png"))
    writePNG(lsimbolo, to: p("altrune-logotipo-simbolo.png"))
    writePNG(lpalabra, to: p("altrune-logotipo-palabra.png"))

    // interface sizes
    let sc = 128.0 / Double(max(simbolo.w, simbolo.h))
    writePNG(resize(simbolo, w: Int((Double(simbolo.w) * sc).rounded()), h: Int((Double(simbolo.h) * sc).rounded())),
             to: p("altrune-simbolo-128.png"))
    let h512 = Int((Double(logotipo.h) * 512.0 / Double(logotipo.w)).rounded())
    writePNG(resize(logotipo, w: 512, h: h512), to: p("altrune-logotipo-512.png"))
    writePNG(resize(lsimbolo, w: 512, h: h512), to: p("altrune-logotipo-simbolo-512.png"))
    writePNG(resize(lpalabra, w: 512, h: h512), to: p("altrune-logotipo-palabra-512.png"))

    // fidelity, measured on the files as written
    let fSym = decode8(p("altrune-simbolo.png")), fAll = decode8(p("altrune-logotipo.png"))
    let fLS = decode8(p("altrune-logotipo-simbolo.png")), fLP = decode8(p("altrune-logotipo-palabra.png"))
    var report = [String]()
    report.append("Background measured (mean of the outer 250 px frame): R \(String(format: "%.3f", e.bg[0])), G \(String(format: "%.3f", e.bg[1])), B \(String(format: "%.3f", e.bg[2]))")
    report.append("Split row between symbol and wordmark: y = \(e.splitRow)")
    report.append("Symbol bbox in the original: x \(bbSym.x0)-\(bbSym.x1), y \(bbSym.y0)-\(bbSym.y1); whole logo bbox: x \(bbAll.x0)-\(bbAll.x1), y \(bbAll.y0)-\(bbAll.y1)")
    report.append("Canvas: altrune-simbolo.png \(simbolo.w)x\(simbolo.h) at (\(sx),\(sy)); altrune-logotipo*.png \(logotipo.w)x\(logotipo.h) at (\(lx),\(ly))")
    let allLayers = [(bytes: fAll.bytes, w: fAll.w, h: fAll.h, ox: lx, oy: ly)]
    report.append("Fidelity (extracted layers, as saved, composited over the measured background and rounded, vs. original):")
    report.append(describe("altrune-logotipo.png, whole 1254x1254 image", compareFidelity(orig: orig, ow: ow, oh: oh, layers: allLayers, bg: e.bg, onlyShape: false)))
    report.append(describe("altrune-logotipo.png, only pixels with alpha > 0", compareFidelity(orig: orig, ow: ow, oh: oh, layers: allLayers, bg: e.bg, onlyShape: true)))
    let symOnly = [(bytes: fSym.bytes, w: fSym.w, h: fSym.h, ox: sx, oy: sy)]
    // symbol alone vs the original, restricted to the symbol rows
    var origSym = orig
    for y in e.splitRow..<oh { for x in 0..<ow { for c in 0..<3 { origSym[(y * ow + x) * 4 + c] = UInt8(max(0, min(255, e.bg[c].rounded()))) } } }
    report.append(describe("altrune-simbolo.png vs. symbol rows (wordmark rows masked to background)", compareFidelity(orig: origSym, ow: ow, oh: oh, layers: symOnly, bg: e.bg, onlyShape: false)))
    // the two split layers must partition the whole logo exactly
    var partitionBad = 0
    for i in 0..<(fAll.w * fAll.h) {
        let a = fAll.bytes[i * 4 + 3], s = fLS.bytes[i * 4 + 3], p = fLP.bytes[i * 4 + 3]
        let y = i / fAll.w + ly
        let wantSym = y < e.splitRow
        let okAlpha = wantSym ? (s == a && p == 0) : (p == a && s == 0)
        var okColour = true
        if wantSym { for c in 0..<3 where fLS.bytes[i * 4 + c] != fAll.bytes[i * 4 + c] { okColour = false } }
        if !(okAlpha && okColour) { partitionBad += 1 }
    }
    report.append("  split layers vs. altrune-logotipo.png: pixels that do not partition exactly (alpha and symbol colour) = \(partitionBad) of \(fAll.w * fAll.h)")
    for l in report { print(l) }
}

// MARK: - Command: icono

/// Apple-style continuous-corner rounded square (the "squircle" used by the
/// macOS icon template): straight sides, then a corner that eases in with a
/// Bezier before a circular arc (corner smoothing 0.6).
func squirclePath(x: Double, y: Double, w: Double, h: Double, radius r: Double, smoothing s: Double) -> CGPath {
    let p = (1 + s) * r
    let arcMeasure = 90.0 * (1 - s)
    let arcSection = sin(arcMeasure / 2 * .pi / 180) * r * 2.0.squareRoot()
    let angleAlpha = (90 - arcMeasure) / 2
    let p3p4 = r * tan(angleAlpha / 2 * .pi / 180)
    let beta = 45 * s
    let c = p3p4 * cos(beta * .pi / 180)
    let d = c * tan(beta * .pi / 180)
    let b = (p - arcSection - c - d) / 3
    let a = 2 * b
    // Top-right corner as three cubic segments relative to its start point.
    let k = 4.0 / 3.0 * tan(arcMeasure / 4 * .pi / 180) * r
    let t0 = beta * .pi / 180, t1 = (beta + arcMeasure) * .pi / 180
    typealias V = (Double, Double)
    let seg1: (V, V, V) = ((a, 0), (a + b, 0), (a + b + c, d))
    let a0: V = (a + b + c, d)
    let a3: V = (a0.0 + arcSection, a0.1 + arcSection)
    let seg2: (V, V, V) = ((a0.0 + k * cos(t0), a0.1 + k * sin(t0)), (a3.0 - k * cos(t1), a3.1 - k * sin(t1)), a3)
    let seg3: (V, V, V) = ((a3.0 + d, a3.1 + c), (a3.0 + d, a3.1 + b + c), (a3.0 + d, a3.1 + a + b + c))
    // seg3 is relative to a3 in the original; rebuild it relative to the corner start
    let s3: (V, V, V) = ((a3.0 + (seg3.0.0 - a3.0), a3.1 + (seg3.0.1 - a3.1)), seg3.1, seg3.2)
    let rel: [(V, V, V)] = [seg1, seg2, s3]
    func rot(_ v: V, _ n: Int) -> V {
        var q = v
        for _ in 0..<n { q = (-q.1, q.0) }
        return q
    }
    let starts: [V] = [(x + w - p, y), (x + w, y + h - p), (x + p, y + h), (x, y + p)]
    let path = CGMutablePath()
    path.move(to: CGPoint(x: starts[0].0, y: starts[0].1))
    for n in 0..<4 {
        if n > 0 { path.addLine(to: CGPoint(x: starts[n].0, y: starts[n].1)) }
        let o = starts[n]
        for sgm in rel {
            let c1 = rot(sgm.0, n), c2 = rot(sgm.1, n), e = rot(sgm.2, n)
            path.addCurve(to: CGPoint(x: o.0 + e.0, y: o.1 + e.1),
                          control1: CGPoint(x: o.0 + c1.0, y: o.1 + c1.1),
                          control2: CGPoint(x: o.0 + c2.0, y: o.1 + c2.1))
        }
    }
    path.closeSubpath()
    return path
}

/// The A cut out of altrune-simbolo.png without its margin, as straight RGBA.
func loadSymbol(_ file: String) -> Img {
    let (w, h, b) = decode8(file)
    var img = Img(w: w, h: h)
    for i in 0..<(w * h) {
        let a = Float(b[i * 4 + 3]) / 255
        img.d[i * 4 + 3] = a
        if a > 0 { for c in 0..<3 { img.d[i * 4 + c] = min(255, Float(b[i * 4 + c]) / a) } }
    }
    let bb = bbox(img)
    return crop(img, x0: bb.x0, y0: bb.y0, w: bb.x1 - bb.x0 + 1, h: bb.y1 - bb.y0 + 1)
}

struct IconLayout { var symbolFraction: Double; var smallVariant: Bool }

/// Vertical centre of the symbol used for optical centring: halfway between
/// the middle of its box and its centre of mass (an A is heavy at the bottom,
/// so centring only the box makes it look low). Measured in symbol pixels from the top.
func opticalCentre(_ sym: Img) -> (box: Double, mass: Double, optical: Double) {
    var sy = 0.0, sa = 0.0
    for y in 0..<sym.h { for x in 0..<sym.w {
        let a = Double(sym.d[(y * sym.w + x) * 4 + 3]); sa += a; sy += a * (Double(y) + 0.5)
    } }
    let box = Double(sym.h) / 2, mass = sy / sa
    return (box, mass, (box + mass) / 2)
}

func composeIcon(size S: Int, sym: Img) -> Img {
    let small = S <= 32
    let fS = Double(S)
    let body: Double, bx: Double
    if small {
        let m = S <= 16 ? 1.0 : 2.0
        body = fS - 2 * m; bx = m
    } else {
        body = fS * 824.0 / 1024.0; bx = (fS - body) / 2
    }
    let by = bx
    var buf = [UInt8](repeating: 0, count: S * S * 4)
    let ctx = CGContext(data: &buf, width: S, height: S, bitsPerComponent: 8, bytesPerRow: S * 4,
                        space: srgb, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.translateBy(x: 0, y: fS); ctx.scaleBy(x: 1, y: -1)
    ctx.setAllowsAntialiasing(true); ctx.setShouldAntialias(true)
    let path = squirclePath(x: bx, y: by, w: body, h: body, radius: body * 0.225, smoothing: 0.6)
    if !small {
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: -fS * 10.0 / 1024.0), blur: fS * 24.0 / 1024.0,
                      color: CGColor(colorSpace: srgb, components: [0, 0, 0, 0.26])!)
        ctx.setFillColor(CGColor(colorSpace: srgb, components: [0.97, 0.98, 0.99, 1])!)
        ctx.addPath(path); ctx.fillPath()
        ctx.restoreGState()
    }
    // Frost body: white to a barely cooler white, top to bottom
    ctx.saveGState()
    ctx.addPath(path); ctx.clip()
    let grad = CGGradient(colorsSpace: srgb,
                          colors: [CGColor(colorSpace: srgb, components: [1.0, 1.0, 1.0, 1])!,
                                   CGColor(colorSpace: srgb, components: [241.0 / 255, 243.0 / 255, 246.0 / 255, 1])!] as CFArray,
                          locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: by), end: CGPoint(x: 0, y: by + body), options: [])
    if small {
        // No shadow at these sizes, so a 1 px inner hairline keeps the body edge
        // visible on white lists and sidebars (clipped to the body: width 2 gives 1 px inside).
        ctx.setStrokeColor(CGColor(colorSpace: srgb, components: [0, 0, 0, 0.14])!)
        ctx.setLineWidth(2)
        ctx.addPath(path); ctx.strokePath()
    }
    ctx.restoreGState()

    // symbol, resampled once from the original-resolution layer
    let frac = S <= 16 ? 0.74 : (small ? 0.70 : 0.55)
    let scale = frac * body / Double(sym.w)
    let sw = Int((Double(sym.w) * scale).rounded()), sh = Int((Double(sym.h) * scale).rounded())
    let rs = resize(sym, w: sw, h: sh)
    let oc = opticalCentre(sym)
    let cx = bx + body / 2, cy = by + body / 2
    let left = Int((cx - Double(sw) / 2).rounded())
    let top = Int((cy - oc.optical * scale).rounded())
    var out = Img(w: S, h: S)
    for y in 0..<S { for x in 0..<S {
        let i = (y * S + x) * 4
        let ba = Float(buf[i + 3]) / 255
        var pr = [Float(buf[i]), Float(buf[i + 1]), Float(buf[i + 2])]
        var pa = ba
        let sx = x - left, sy = y - top
        if sx >= 0 && sy >= 0 && sx < sw && sy < sh {
            let j = (sy * sw + sx) * 4
            let a = rs.d[j + 3]
            for c in 0..<3 { pr[c] = rs.d[j + c] * a + (1 - a) * pr[c] }
            pa = a + (1 - a) * ba
        }
        out.d[i + 3] = pa
        if pa > 0 { for c in 0..<3 { out.d[i + c] = min(255, pr[c] / pa) } }
    } }
    return out
}

func cmdIcono(_ out: String, _ iconset: String) {
    let sym = loadSymbol(out + "/altrune-simbolo.png")
    let oc = opticalCentre(sym)
    print(String(format: "symbol %dx%d; vertical centre: box %.1f, mass %.1f, optical %.1f (nudge up %.1f px at 1x)",
                 sym.w, sym.h, oc.box, oc.mass, oc.optical, oc.optical - oc.box))
    let master = composeIcon(size: 1024, sym: sym)
    writePNG(master, to: out + "/AppIcon-1024.png")
    print(String(format: "master: symbol drawn at x%.3f of its original size", 0.55 * 824.0 / Double(sym.w)))
    let table: [(String, Int)] = [("icon_16x16", 16), ("icon_16x16@2x", 32), ("icon_32x32", 32), ("icon_32x32@2x", 64),
                                  ("icon_128x128", 128), ("icon_128x128@2x", 256), ("icon_256x256", 256),
                                  ("icon_256x256@2x", 512), ("icon_512x512", 512), ("icon_512x512@2x", 1024)]
    for (name, px) in table {
        writePNG(px == 1024 ? master : composeIcon(size: px, sym: sym), to: iconset + "/" + name + ".png")
    }
}

// MARK: - Command: paleta

func hexOf(_ c: [Double]) -> String {
    String(format: "#%02X%02X%02X", Int(c[0].rounded()), Int(c[1].rounded()), Int(c[2].rounded()))
}
func relLuminance(_ hex: String) -> Double {
    let v = hex.dropFirst()
    let ch = (0..<3).map { i -> Double in
        let s = v.index(v.startIndex, offsetBy: i * 2)
        let c = Double(Int(v[s..<v.index(s, offsetBy: 2)], radix: 16)!) / 255
        return c <= 0.03928 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
    }
    return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2]
}
func contrast(_ a: String, _ b: String) -> Double {
    let la = relLuminance(a), lb = relLuminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
}

func cmdPaleta(_ logo: String) {
    let (w, _, px) = decode8(logo)
    func patch(_ x: Int, _ y: Int) -> [Double] {
        var s = [0.0, 0, 0]
        for dy in -2...2 { for dx in -2...2 { let i = ((y + dy) * w + x + dx) * 4; for c in 0..<3 { s[c] += Double(px[i + c]) } } }
        return s.map { $0 / 25 }
    }
    let regions: [(String, [(Int, Int)])] = [
        ("Left leg, bottom (sky blue)", [(480, 675), (500, 685), (520, 688)]),
        ("Left leg, middle", [(500, 640), (520, 620), (540, 595)]),
        ("Left leg, top (lighter blue)", [(600, 435), (590, 470), (610, 460)]),
        ("Fold at the apex", [(650, 428), (665, 440), (680, 455)]),
        ("Indigo overlap, top (darkest)", [(650, 545), (690, 545), (670, 555)]),
        ("Indigo overlap, centre", [(680, 580), (700, 600), (660, 590)]),
        ("Right leg, top (light violet)", [(700, 500), (680, 470), (715, 520)]),
        ("Right leg, bottom (violet)", [(780, 690), (760, 685), (770, 670)]),
    ]
    print("Symbol samples (mean of 5x5 pixels at original coordinates x,y):")
    for (name, pts) in regions {
        let cols = pts.map { patch($0.0, $0.1) }
        let mean = (0..<3).map { c in cols.map { $0[c] }.reduce(0, +) / Double(cols.count) }
        let detail = zip(pts, cols).map { "(\($0.0),\($0.1)) \(hexOf($1))" }.joined(separator: ", ")
        print("  \(name): mean \(hexOf(mean))  [\(detail)]")
    }
    // wordmark ink: median of solid ink pixels (all 5x5 neighbours dark)
    var inks = [[Int]](repeating: [], count: 3)
    for y in 750..<849 { for x in 330..<925 {
        var ok = true
        for dy in -2...2 { for dx in -2...2 { let i = ((y + dy) * w + x + dx) * 4; if px[i] > 110 || px[i + 1] > 110 || px[i + 2] > 110 { ok = false } } }
        if ok { let i = (y * w + x) * 4; for c in 0..<3 { inks[c].append(Int(px[i + c])) } }
    } }
    let med = inks.map { v -> Double in let s = v.sorted(); return Double(s[s.count / 2]) }
    print("Wordmark ink: median of \(inks[0].count) solid pixels = \(hexOf(med)); R range \(inks[0].min()!)-\(inks[0].max()!), G \(inks[1].min()!)-\(inks[1].max()!), B \(inks[2].min()!)-\(inks[2].max()!)")

    // Proposed interface tokens and their WCAG 2.x contrast ratios. Each token
    // lists what it is checked against and the minimum ratio for that use
    // (4.5 for text, 3 for non-text boundaries such as a button edge or a focus ring).
    typealias Check = (against: String, label: String, need: Double)
    let ls: [Check] = [("#F5F6F8", "light surface", 4.5), ("#FFFFFF", "white card", 4.5)]
    let ds: [Check] = [("#1C1E22", "dark surface", 4.5), ("#26282D", "dark card", 4.5)]
    func nonText(_ c: [Check]) -> [Check] { c.map { ($0.against, $0.label, 3.0) } }
    let toks: [(String, String, [Check])] = [
        ("light button fill", "#2F5FE3", [("#FFFFFF", "white text", 4.5)] + nonText(ls)),
        ("light accent text", "#2A55D2", ls),
        ("light focus / selection ring", "#2F6BFF", nonText(ls)),
        ("light selection background", "#D6E2FF", [("#15191F", "ink text", 4.5), ("#2A55D2", "accent text", 4.5)]),
        ("dark button fill", "#3D68F5", [("#FFFFFF", "white text", 4.5)] + nonText(ds)),
        ("dark accent text", "#7AA5FF", ds),
        ("dark focus / selection ring", "#5B8CFF", nonText(ds)),
        ("dark selection background", "#263870", [("#FFFFFF", "white text", 4.5), ("#7AA5FF", "accent text", 4.5), ("#5B8CFF", "focus ring", 3.0)]),
    ]
    print("Contrast ratios (WCAG 2.x relative luminance):")
    for (name, hex, checks) in toks {
        let parts = checks.map { c -> String in
            let r = contrast(hex, c.against)
            return String(format: "%@ %@ %.2f:1 (min %.1f) %@", c.label, c.against, r, c.need, r >= c.need ? "ok" : "FAIL")
        }
        print("  \(name) \(hex): " + parts.joined(separator: "; "))
    }
}

// MARK: - Dispatch

let args = CommandLine.arguments
guard args.count >= 2 else { die("usage: marca.swift extraer|icono|paleta ...") }
switch args[1] {
case "extraer": cmdExtraer(args[2], args[3])
case "icono": cmdIcono(args[2], args[3])
case "paleta": cmdPaleta(args[2])
default: die("unknown command \(args[1])")
}
