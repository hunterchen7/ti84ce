//
//  DPadView.swift
//  Calc
//
//  Circular D-pad navigation control with 4 directional segments.
//

import SwiftUI

/// Direction for D-pad
enum DPadDirection {
    case up, down, left, right

    /// CEmu key matrix coordinates
    var keyCoords: (row: Int32, col: Int32) {
        switch self {
        case .up: return (7, 3)
        case .down: return (7, 0)
        case .left: return (7, 1)
        case .right: return (7, 2)
        }
    }

    /// Angle in degrees for this direction
    var angle: CGFloat {
        switch self {
        case .up: return 270
        case .down: return 90
        case .left: return 180
        case .right: return 0
        }
    }
}

/// Circular D-pad with 4 directional segments
struct DPadView: View {
    let onKeyDown: (Int32, Int32) -> Void
    let onKeyUp: (Int32, Int32) -> Void

    @State private var pressedDirection: DPadDirection?

    // Configuration
    private let sweepAngle: CGFloat = 90
    private let innerRadiusScale: CGFloat = 0.45
    private let gapWidthScale: CGFloat = 0.16

    // Colors
    private let segmentColor = Color(red: 0.890, green: 0.890, blue: 0.890) // #E3E3E3
    private let pressedColor = Color(red: 0.808, green: 0.808, blue: 0.808) // #CECECE
    private let borderColor = Color(red: 0.710, green: 0.710, blue: 0.710) // #B5B5B5
    private let arrowColor = Color(red: 0.169, green: 0.169, blue: 0.169) // #2B2B2B
    private let gapColor = Color(red: 0.106, green: 0.106, blue: 0.106) // #1B1B1B
    private let centerColor = Color(red: 0.106, green: 0.106, blue: 0.106) // #1B1B1B

    var body: some View {
        GeometryReader { geometry in
            let size = min(geometry.size.width, geometry.size.height)

            ZStack {
                // Draw segments
                ForEach([DPadDirection.up, .down, .left, .right], id: \.self) { direction in
                    DPadSegment(
                        direction: direction,
                        sweepAngle: sweepAngle,
                        innerRadiusScale: innerRadiusScale,
                        gapWidthScale: gapWidthScale,
                        fillColor: segmentColor,
                        pressedColor: pressedColor,
                        borderColor: borderColor,
                        arrowColor: arrowColor,
                        isPressed: pressedDirection == direction
                    )
                }

                // Draw gaps
                DPadGaps(gapWidthScale: gapWidthScale, color: gapColor)

                // Center circle with dot
                ZStack {
                    Circle()
                        .fill(centerColor)
                    Circle()
                        .fill(Color(red: 0.35, green: 0.35, blue: 0.35))
                        .frame(width: size * 0.08, height: size * 0.08)
                }
                .frame(width: size * 0.25, height: size * 0.25)
            }
            .frame(width: size, height: size)
            .contentShape(Circle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        let hitSize = CGSize(width: size, height: size)

                        if let hit = hitTestDPad(
                            point: value.location,
                            size: hitSize,
                            sweepAngle: sweepAngle,
                            innerRadiusScale: innerRadiusScale,
                            gapWidthScale: gapWidthScale
                        ) {
                            if pressedDirection != hit {
                                // Release previous
                                if let prev = pressedDirection {
                                    onKeyUp(prev.keyCoords.row, prev.keyCoords.col)
                                }
                                // Press new
                                pressedDirection = hit
                                onKeyDown(hit.keyCoords.row, hit.keyCoords.col)
                            }
                        } else {
                            // Outside valid area
                            if let prev = pressedDirection {
                                onKeyUp(prev.keyCoords.row, prev.keyCoords.col)
                                pressedDirection = nil
                            }
                        }
                    }
                    .onEnded { _ in
                        if let prev = pressedDirection {
                            onKeyUp(prev.keyCoords.row, prev.keyCoords.col)
                            pressedDirection = nil
                        }
                    }
            )
            .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
        }
    }

    /// Hit test for D-pad segments
    private func hitTestDPad(
        point: CGPoint,
        size: CGSize,
        sweepAngle: CGFloat,
        innerRadiusScale: CGFloat,
        gapWidthScale: CGFloat
    ) -> DPadDirection? {
        let directions: [DPadDirection] = [.up, .left, .right, .down]

        for direction in directions {
            let startAngle = direction.angle - sweepAngle / 2
            if isPointInSegment(
                point: point,
                size: size,
                startAngle: startAngle,
                sweepAngle: sweepAngle,
                innerRadiusScale: innerRadiusScale,
                gapWidthScale: gapWidthScale
            ) {
                return direction
            }
        }
        return nil
    }

    /// Check if point is within a D-pad segment
    private func isPointInSegment(
        point: CGPoint,
        size: CGSize,
        startAngle: CGFloat,
        sweepAngle: CGFloat,
        innerRadiusScale: CGFloat,
        gapWidthScale: CGFloat
    ) -> Bool {
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        let dx = point.x - center.x
        let dy = point.y - center.y
        let radius = sqrt(dx * dx + dy * dy)

        let outerRadius = min(size.width, size.height) / 2
        let innerRadius = outerRadius * innerRadiusScale

        // Check radius bounds
        guard radius >= innerRadius && radius <= outerRadius else {
            return false
        }

        // Check if in gap
        let gapWidth = outerRadius * gapWidthScale
        let threshold = gapWidth * 0.5 * 1.41421356
        if abs(dy - dx) < threshold || abs(dy + dx) < threshold {
            return false
        }

        // Check angle
        var angle = atan2(dy, dx) * 180 / .pi
        if angle < 0 { angle += 360 }

        var start = startAngle
        if start < 0 { start += 360 }
        var end = start + sweepAngle
        if end >= 360 { end -= 360 }

        if start <= end {
            return angle >= start && angle <= end
        } else {
            return angle >= start || angle <= end
        }
    }
}

/// Single D-pad segment
struct DPadSegment: View {
    let direction: DPadDirection
    let sweepAngle: CGFloat
    let innerRadiusScale: CGFloat
    let gapWidthScale: CGFloat
    let fillColor: Color
    let pressedColor: Color
    let borderColor: Color
    let arrowColor: Color
    let isPressed: Bool

    var body: some View {
        Canvas { context, size in
            let outerRadius = min(size.width, size.height) / 2
            let innerRadius = outerRadius * innerRadiusScale
            let strokeWidth = outerRadius * 0.035
            let center = CGPoint(x: size.width / 2, y: size.height / 2)

            let outerRadiusAdj = outerRadius - strokeWidth * 0.2
            let innerRadiusAdj = innerRadius + strokeWidth * 0.15

            let startAngle = Angle(degrees: direction.angle - sweepAngle / 2)
            let endAngle = Angle(degrees: direction.angle + sweepAngle / 2)

            // Create segment path
            var path = Path()
            path.addArc(center: center, radius: outerRadiusAdj, startAngle: startAngle, endAngle: endAngle, clockwise: false)
            path.addArc(center: center, radius: innerRadiusAdj, startAngle: endAngle, endAngle: startAngle, clockwise: true)
            path.closeSubpath()

            // Fill
            let activeFill = isPressed ? pressedColor : fillColor
            context.fill(path, with: .color(activeFill))

            // Border
            let rimColor = isPressed
                ? borderColor.blended(with: .black, ratio: 0.35)
                : borderColor.blended(with: .white, ratio: 0.35)
            context.stroke(path, with: .color(rimColor), lineWidth: strokeWidth)

            // Inner rim
            let innerRim = isPressed
                ? activeFill.blended(with: .black, ratio: 0.15)
                : activeFill.blended(with: .white, ratio: 0.18)
            context.stroke(path, with: .color(innerRim), lineWidth: strokeWidth * 0.6)

            // Draw arrow
            let arrowRadius = (innerRadius + outerRadius) * 0.5
            let angleRad = direction.angle * .pi / 180
            let arrowCenter = CGPoint(
                x: center.x + cos(angleRad) * arrowRadius,
                y: center.y + sin(angleRad) * arrowRadius
            )

            let arrowLength = outerRadius * 0.09
            let arrowWidth = outerRadius * 0.16

            let forward = CGPoint(x: cos(angleRad), y: sin(angleRad))
            let perpendicular = CGPoint(x: -forward.y, y: forward.x)

            let tip = CGPoint(
                x: arrowCenter.x + forward.x * arrowLength,
                y: arrowCenter.y + forward.y * arrowLength
            )
            let baseCenter = CGPoint(
                x: arrowCenter.x - forward.x * (arrowLength * 0.45),
                y: arrowCenter.y - forward.y * (arrowLength * 0.45)
            )
            let left = CGPoint(
                x: baseCenter.x + perpendicular.x * (arrowWidth * 0.5),
                y: baseCenter.y + perpendicular.y * (arrowWidth * 0.5)
            )
            let right = CGPoint(
                x: baseCenter.x - perpendicular.x * (arrowWidth * 0.5),
                y: baseCenter.y - perpendicular.y * (arrowWidth * 0.5)
            )

            var arrowPath = Path()
            arrowPath.move(to: tip)
            arrowPath.addLine(to: left)
            arrowPath.addLine(to: right)
            arrowPath.closeSubpath()

            context.fill(arrowPath, with: .color(arrowColor))
        }
    }
}

/// D-pad gap lines
struct DPadGaps: View {
    let gapWidthScale: CGFloat
    let color: Color

    var body: some View {
        Canvas { context, size in
            let outerRadius = min(size.width, size.height) / 2
            let gapWidth = outerRadius * gapWidthScale
            let rectLength = outerRadius * 2.1
            let center = CGPoint(x: size.width / 2, y: size.height / 2)

            let rectSize = CGSize(width: gapWidth, height: rectLength)

            // 45 degree gap
            context.drawLayer { ctx in
                ctx.translateBy(x: center.x, y: center.y)
                ctx.rotate(by: .degrees(45))
                ctx.translateBy(x: -center.x, y: -center.y)
                ctx.fill(
                    Path(CGRect(
                        x: center.x - gapWidth / 2,
                        y: center.y - rectLength / 2,
                        width: rectSize.width,
                        height: rectSize.height
                    )),
                    with: .color(color)
                )
            }

            // -45 degree gap
            context.drawLayer { ctx in
                ctx.translateBy(x: center.x, y: center.y)
                ctx.rotate(by: .degrees(-45))
                ctx.translateBy(x: -center.x, y: -center.y)
                ctx.fill(
                    Path(CGRect(
                        x: center.x - gapWidth / 2,
                        y: center.y - rectLength / 2,
                        width: rectSize.width,
                        height: rectSize.height
                    )),
                    with: .color(color)
                )
            }
        }
    }
}

#Preview {
    DPadView(
        onKeyDown: { row, col in print("Down: \(row), \(col)") },
        onKeyUp: { row, col in print("Up: \(row), \(col)") }
    )
    .frame(width: 120, height: 120)
    .padding()
    .background(Color(red: 0.106, green: 0.106, blue: 0.106))
}
