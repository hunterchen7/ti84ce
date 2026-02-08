//
//  ImageKeypadView.swift
//  Calc
//
//  Image-based keypad using real TI-84 Plus CE button photos.
//

import SwiftUI

/// Button region data (percentages of keypad body image)
struct ButtonRegion {
    let name: String
    let row: Int32
    let col: Int32
    let left: CGFloat   // percentage
    let top: CGFloat    // percentage
    let width: CGFloat  // percentage
    let height: CGFloat // percentage
    let img: String     // image filename (without extension)
}

/// Keypad aspect ratio matching the extracted body image
private let keypadAspectRatio: CGFloat = 0.657338

/// All button regions extracted from hires-ti84ce-cropped.png (coordinates as percentages of keypad body)
private let buttonRegions: [ButtonRegion] = [
    // Function row
    ButtonRegion(name: "y=",     row: 1, col: 4, left: 11.32, top:  5.12, width: 11.01, height: 4.03, img: "btn_y_eq"),
    ButtonRegion(name: "window", row: 1, col: 3, left: 27.73, top:  5.12, width: 11.32, height: 4.03, img: "btn_window"),
    ButtonRegion(name: "zoom",   row: 1, col: 2, left: 44.76, top:  5.12, width: 10.80, height: 4.03, img: "btn_zoom"),
    ButtonRegion(name: "trace",  row: 1, col: 1, left: 61.37, top:  5.12, width: 10.70, height: 4.03, img: "btn_trace"),
    ButtonRegion(name: "graph",  row: 1, col: 0, left: 77.78, top:  5.12, width: 11.01, height: 4.03, img: "btn_graph"),
    // Control row 1
    ButtonRegion(name: "2nd",    row: 1, col: 5, left: 11.32, top: 14.20, width: 11.01, height: 5.32, img: "btn_2nd"),
    ButtonRegion(name: "mode",   row: 1, col: 6, left: 27.73, top: 14.20, width: 11.01, height: 5.32, img: "btn_mode"),
    ButtonRegion(name: "del",    row: 1, col: 7, left: 44.65, top: 14.20, width: 11.01, height: 5.32, img: "btn_del"),
    // Control row 2
    ButtonRegion(name: "alpha",  row: 2, col: 7, left: 11.32, top: 22.73, width: 11.01, height: 5.32, img: "btn_alpha"),
    ButtonRegion(name: "xttn",   row: 3, col: 7, left: 27.73, top: 22.53, width: 11.01, height: 5.53, img: "btn_xttn"),
    ButtonRegion(name: "stat",   row: 4, col: 7, left: 44.65, top: 22.73, width: 11.01, height: 5.32, img: "btn_stat"),
    // Math row
    ButtonRegion(name: "math",   row: 2, col: 6, left: 11.11, top: 31.13, width: 11.11, height: 5.46, img: "btn_math"),
    ButtonRegion(name: "apps",   row: 3, col: 6, left: 28.04, top: 31.13, width: 10.70, height: 5.46, img: "btn_apps"),
    ButtonRegion(name: "prgm",   row: 4, col: 6, left: 44.65, top: 31.19, width: 11.01, height: 5.32, img: "btn_prgm"),
    ButtonRegion(name: "vars",   row: 5, col: 6, left: 61.37, top: 31.19, width: 10.70, height: 5.32, img: "btn_vars"),
    ButtonRegion(name: "clear",  row: 6, col: 6, left: 77.47, top: 31.19, width: 11.32, height: 5.46, img: "btn_clear"),
    // Trig row
    ButtonRegion(name: "x_inv",  row: 2, col: 5, left: 11.01, top: 39.66, width: 11.32, height: 5.67, img: "btn_x_inv"),
    ButtonRegion(name: "sin",    row: 3, col: 5, left: 28.04, top: 39.66, width: 10.70, height: 5.32, img: "btn_sin"),
    ButtonRegion(name: "cos",    row: 4, col: 5, left: 44.65, top: 39.66, width: 11.01, height: 5.32, img: "btn_cos"),
    ButtonRegion(name: "tan",    row: 5, col: 5, left: 61.37, top: 39.66, width: 10.70, height: 5.32, img: "btn_tan"),
    ButtonRegion(name: "pow",    row: 6, col: 5, left: 77.78, top: 39.66, width: 11.01, height: 5.46, img: "btn_pow"),
    // Special row
    ButtonRegion(name: "x_sq",   row: 2, col: 4, left: 11.32, top: 48.19, width: 11.01, height: 5.32, img: "btn_x_sq"),
    ButtonRegion(name: "comma",  row: 3, col: 4, left: 28.04, top: 48.19, width: 10.70, height: 5.32, img: "btn_comma"),
    ButtonRegion(name: "lparen", row: 4, col: 4, left: 44.65, top: 48.40, width: 11.01, height: 5.12, img: "btn_lparen"),
    ButtonRegion(name: "rparen", row: 5, col: 4, left: 61.37, top: 48.40, width: 10.70, height: 5.12, img: "btn_rparen"),
    ButtonRegion(name: "div",    row: 6, col: 4, left: 77.67, top: 48.19, width: 11.11, height: 5.32, img: "btn_div"),
    // Number block row 1
    ButtonRegion(name: "log",    row: 2, col: 3, left: 11.32, top: 56.93, width: 11.01, height: 5.12, img: "btn_log"),
    ButtonRegion(name: "7",      row: 3, col: 3, left: 27.93, top: 56.79, width: 11.01, height: 6.21, img: "btn_7"),
    ButtonRegion(name: "8",      row: 4, col: 3, left: 44.65, top: 56.66, width: 11.01, height: 6.21, img: "btn_8"),
    ButtonRegion(name: "9",      row: 5, col: 3, left: 61.16, top: 56.93, width: 11.01, height: 6.21, img: "btn_9"),
    ButtonRegion(name: "mul",    row: 6, col: 3, left: 77.78, top: 56.79, width: 11.01, height: 5.19, img: "btn_mul"),
    // Number block row 2
    ButtonRegion(name: "ln",     row: 2, col: 2, left: 11.32, top: 65.12, width: 11.11, height: 5.53, img: "btn_ln"),
    ButtonRegion(name: "4",      row: 3, col: 2, left: 27.93, top: 66.35, width: 11.01, height: 6.21, img: "btn_4"),
    ButtonRegion(name: "5",      row: 4, col: 2, left: 44.65, top: 66.35, width: 11.01, height: 6.21, img: "btn_5"),
    ButtonRegion(name: "6",      row: 5, col: 2, left: 61.16, top: 66.35, width: 11.01, height: 6.21, img: "btn_6"),
    ButtonRegion(name: "sub",    row: 6, col: 2, left: 77.78, top: 65.39, width: 11.01, height: 5.12, img: "btn_sub"),
    // Number block row 3
    ButtonRegion(name: "sto",    row: 2, col: 1, left: 11.01, top: 73.52, width: 11.63, height: 5.67, img: "btn_sto"),
    ButtonRegion(name: "1",      row: 3, col: 1, left: 27.93, top: 76.04, width: 11.01, height: 6.21, img: "btn_1"),
    ButtonRegion(name: "2",      row: 4, col: 1, left: 44.65, top: 76.04, width: 11.01, height: 6.21, img: "btn_2"),
    ButtonRegion(name: "3",      row: 5, col: 1, left: 61.16, top: 75.84, width: 11.01, height: 6.21, img: "btn_3"),
    ButtonRegion(name: "add",    row: 6, col: 1, left: 77.78, top: 73.58, width: 11.01, height: 5.32, img: "btn_add"),
    // Number block row 4
    ButtonRegion(name: "on",     row: 2, col: 0, left: 11.11, top: 81.91, width: 11.11, height: 5.94, img: "btn_on"),
    ButtonRegion(name: "0",      row: 3, col: 0, left: 27.93, top: 85.80, width: 11.01, height: 6.21, img: "btn_0"),
    ButtonRegion(name: "dot",    row: 4, col: 0, left: 44.65, top: 85.80, width: 11.01, height: 6.21, img: "btn_dot"),
    ButtonRegion(name: "neg",    row: 5, col: 0, left: 61.16, top: 85.80, width: 11.01, height: 6.21, img: "btn_neg"),
    ButtonRegion(name: "enter",  row: 6, col: 0, left: 77.78, top: 82.39, width: 11.01, height: 5.12, img: "btn_enter"),
]

/// Image-based keypad using real calculator photos
struct ImageKeypadView: View {
    let onKeyDown: (Int32, Int32) -> Void
    let onKeyUp: (Int32, Int32) -> Void

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            let height = geometry.size.height

            ZStack(alignment: .topLeading) {
                // Button overlays (no background — parent shows combined image)
                ForEach(buttonRegions, id: \.name) { region in
                    ImageKeyButton(
                        region: region,
                        containerWidth: width,
                        containerHeight: height,
                        onDown: { onKeyDown(region.row, region.col) },
                        onUp: { onKeyUp(region.row, region.col) }
                    )
                }

                // D-pad
                DPadView(onKeyDown: onKeyDown, onKeyUp: onKeyUp)
                    .frame(
                        width: width * 22.01 / 100,
                        height: height * 14.74 / 100
                    )
                    .contentShape(Rectangle())
                    .position(
                        x: width * (63.97 + 22.01 / 2) / 100,
                        y: height * (13.72 + 14.74 / 2) / 100
                    )
            }
            .frame(width: width, height: height)
            .clipped()
        }
    }
}

/// Single image-based button with press travel animation
private struct ImageKeyButton: View {
    let region: ButtonRegion
    let containerWidth: CGFloat
    let containerHeight: CGFloat
    let onDown: () -> Void
    let onUp: () -> Void

    @State private var isPressed = false

    private let travelDistance: CGFloat = 2

    var body: some View {
        let x = containerWidth * region.left / 100
        let y = containerHeight * region.top / 100
        let w = containerWidth * region.width / 100
        let h = containerHeight * region.height / 100

        Image(region.img)
            .resizable()
            .aspectRatio(contentMode: .fill)
            .frame(width: w, height: h)
            .clipped()
            .offset(y: isPressed ? travelDistance : 0)
            .brightness(isPressed ? -0.15 : 0)
            .animation(.easeOut(duration: 0.05), value: isPressed)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !isPressed {
                            isPressed = true
                            onDown()
                        }
                    }
                    .onEnded { _ in
                        isPressed = false
                        onUp()
                    }
            )
            .position(x: x + w / 2, y: y + h / 2)
    }
}

// Extension to make ForEach work with ButtonRegion
extension ButtonRegion: Hashable {
    static func == (lhs: ButtonRegion, rhs: ButtonRegion) -> Bool {
        lhs.name == rhs.name
    }
    func hash(into hasher: inout Hasher) {
        hasher.combine(name)
    }
}
