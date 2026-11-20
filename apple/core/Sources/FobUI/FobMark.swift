// Fob's mark (the boards' wallet switcher, 26pt beside the wallet's name):
// an orange pebble with a darker ring and a six-pointed star round a dark
// hexagon, in the SVG's own viewBox (9.91 11 81.28 80.64). Its colours are
// the brand's, the same in every theme.

import SwiftUI

public struct FobMark: View {
    public init() {}

    public var body: some View {
        Canvas { context, size in
            let box = CGRect(x: 9.91, y: 11, width: 81.28, height: 80.64)
            let scale = min(size.width / box.width, size.height / box.height)
            context.translateBy(x: (size.width - box.width * scale) / 2, y: (size.height - box.height * scale) / 2)
            context.scaleBy(x: scale, y: scale)
            context.translateBy(x: -box.minX, y: -box.minY)
            context.fill(Self.polygon(Self.pebble), with: .color(Self.orange))
            context.stroke(Path(ellipseIn: CGRect(x: 21, y: 21, width: 58, height: 58)),
                           with: .color(Self.ember), lineWidth: 3.2)
            for point in Self.star {
                context.fill(Self.polygon(point), with: .color(Self.ember))
            }
            context.fill(Self.polygon(Self.hexagon), with: .color(Self.core))
        }
        .aspectRatio(81.28 / 80.64, contentMode: .fit)
        .accessibilityHidden(true)
    }

    nonisolated private static let orange = Color(argb: 0xFFFF6600)
    nonisolated private static let ember = Color(argb: 0xFFC24E00)
    nonisolated private static let core = Color(argb: 0xFF1A1917)

    nonisolated private static func polygon(_ points: [(CGFloat, CGFloat)]) -> Path {
        var path = Path()
        path.addLines(points.map { CGPoint(x: $0.0, y: $0.1) })
        path.closeSubpath()
        return path
    }

    nonisolated private static let hexagon: [(CGFloat, CGFloat)] = [
        (50, 39.75), (58.88, 44.88), (58.88, 55.12), (50, 60.25), (41.12, 55.12), (41.12, 44.88),
    ]

    /// The star's six points, each a triangle on one of the hexagon's sides.
    nonisolated private static let star: [[(CGFloat, CGFloat)]] = [
        [(50, 39.75), (58.88, 44.88), (61.27, 30.46)],
        [(58.88, 44.88), (58.88, 55.12), (72.55, 50)],
        [(58.88, 55.12), (50, 60.25), (61.27, 69.54)],
        [(50, 60.25), (41.12, 55.12), (38.73, 69.54)],
        [(41.12, 55.12), (41.12, 44.88), (27.45, 50)],
        [(41.12, 44.88), (50, 39.75), (38.73, 30.46)],
    ]

    nonisolated private static let pebble: [(CGFloat, CGFloat)] = [
        (90.67, 50), (91.09, 52.15), (91.19, 54.33), (90.96, 56.49), (90.44, 58.59), (89.66, 60.63),
        (88.69, 62.57), (87.62, 64.44), (86.47, 66.24), (85.3, 67.99), (84.11, 69.69), (82.88, 71.35),
        (81.6, 72.96), (80.23, 74.48), (78.78, 75.91), (77.23, 77.23), (75.61, 78.45), (73.95, 79.58),
        (72.28, 80.66), (70.62, 81.75), (68.99, 82.88), (67.37, 84.09), (65.75, 85.38), (64.1, 86.72),
        (62.36, 88.05), (60.53, 89.29), (58.57, 90.34), (56.51, 91.11), (54.37, 91.56), (52.18, 91.64),
        (50, 91.39), (47.86, 90.84), (45.79, 90.09), (43.79, 89.2), (41.86, 88.27), (39.99, 87.36),
        (38.14, 86.5), (36.31, 85.68), (34.47, 84.88), (32.65, 84.06), (30.84, 83.18), (29.08, 82.22),
        (27.37, 81.14), (25.73, 79.97), (24.15, 78.71), (22.61, 77.39), (21.08, 76.04), (19.55, 74.66),
        (17.99, 73.25), (16.43, 71.8), (14.9, 70.27), (13.45, 68.62), (12.17, 66.84), (11.13, 64.92),
        (10.39, 62.87), (9.98, 60.72), (9.91, 58.52), (10.12, 56.32), (10.54, 54.15), (11.09, 52.04),
        (11.67, 50), (12.19, 48.02), (12.62, 46.07), (12.94, 44.13), (13.16, 42.17), (13.35, 40.18),
        (13.56, 38.16), (13.86, 36.13), (14.3, 34.11), (14.91, 32.12), (15.69, 30.19), (16.63, 28.33),
        (17.68, 26.52), (18.84, 24.77), (20.07, 23.05), (21.37, 21.37), (22.75, 19.74), (24.23, 18.18),
        (25.84, 16.75), (27.59, 15.49), (29.49, 14.47), (31.51, 13.7), (33.62, 13.21), (35.79, 12.97),
        (37.95, 12.92), (40.08, 12.98), (42.15, 13.06), (44.15, 13.08), (46.11, 12.99), (48.05, 12.75),
        (50, 12.39), (51.99, 11.95), (54.04, 11.52), (56.15, 11.18), (58.29, 11), (60.44, 11.05),
        (62.55, 11.36), (64.62, 11.92), (66.6, 12.71), (68.51, 13.67), (70.34, 14.77), (72.11, 15.96),
        (73.81, 17.23), (75.45, 18.57), (77.01, 20), (78.47, 21.53), (79.79, 23.17), (80.94, 24.94),
        (81.91, 26.82), (82.69, 28.77), (83.31, 30.77), (83.84, 32.76), (84.34, 34.71), (84.89, 36.61),
        (85.53, 38.46), (86.29, 40.28), (87.18, 42.1), (88.14, 43.96), (89.11, 45.89), (89.98, 47.9),
    ]
}
