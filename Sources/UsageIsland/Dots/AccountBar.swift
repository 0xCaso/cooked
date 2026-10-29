/// The 5h bar of one account: 20 dots, one per 5%.
enum AccountBar {
    static let cols = 20

    static func grid(percent: Int, hot: Bool) -> DotGrid {
        let p = min(100, max(0, percent))
        let lit = p > 0 ? max(1, Int((Double(p) / 5).rounded())) : 0
        return DotGrid(cols: cols, rows: 1) { c, _ in (c < lit ? 1 : 0, hot) }
    }
}
