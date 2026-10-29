import Foundation

/// A fixed grid of LED dots. Each dot has a continuous level (0 off, 1 fully lit) and may be red.
/// Index is `c * rows + r`, row 0 at the top.
struct DotGrid {
    let cols: Int
    let rows: Int
    var level: [Float]
    var hot: [Bool]

    init(cols: Int, rows: Int, _ f: (Int, Int) -> (Float, Bool)) {
        self.cols = cols
        self.rows = rows
        level = Array(repeating: 0, count: cols * rows)
        hot = Array(repeating: false, count: cols * rows)
        for c in 0..<cols {
            for r in 0..<rows {
                (level[c * rows + r], hot[c * rows + r]) = f(c, r)
            }
        }
    }
}

/// How one dot is drawn: opacity, radius (scaled), how much of the pre-drawn glow shows, and its colour.
struct DotShade: Equatable {
    let alpha: Float
    let radiusScale: Float
    let glow: Float
    let hot: Bool
}

func smoothstep(_ a: Float, _ b: Float, _ v: Float) -> Float {
    let t = min(1, max(0, (v - a) / (b - a)))
    return t * t * (3 - 2 * t)
}

/// The mockup's `fluid()` renderer as numbers (src/lib.js): every value is continuous, so dots fade instead of switching.
enum DotShading {
    static func shade(_ g: DotGrid, floor: (Int) -> Float, vignette: Bool) -> [DotShade] {
        var out: [DotShade] = []
        out.reserveCapacity(g.level.count)
        let neighbours = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        for c in 0..<g.cols {
            for r in 0..<g.rows {
                let i = c * g.rows + r
                var v = g.level[i]
                var hot = g.hot[i]
                // a lit neighbour lifts this dot a little, continuously
                var halo: Float = 0
                for (dc, dr) in neighbours {
                    let nc = c + dc, nr = r + dr
                    guard nc >= 0, nc < g.cols, nr >= 0, nr < g.rows else { continue }
                    let n = nc * g.rows + nr
                    let h = 0.16 * smoothstep(0.6, 0.95, g.level[n])
                    if h > halo {
                        halo = h
                        if g.hot[n] && v < 0.16 { hot = true }
                    }
                }
                v = max(v, halo)
                let rim: Float = vignette ? min(1, Float(min(c, g.cols - 1 - c) + 1) / 4) : 1
                let f = floor(r)
                out.append(DotShade(
                    alpha: (f + v * (0.98 - f)) * rim,
                    radiusScale: 0.8 + 0.2 * smoothstep(0.4, 0.8, v),
                    glow: smoothstep(0.7, 1, v),
                    hot: hot && v > 0.05))
            }
        }
        return out
    }
}
