import SwiftUI

/// hex's springs (rune `Spring::new(duration, bounce 0)`): no bounce, durations are settle times (mockup look.html).
enum Motion {
    enum Duration {
        static let press = 0.08
        static let release = 0.18
        static let control = 0.2
        static let popoverIn = 0.24
        static let popoverOut = 0.2
    }

    enum Scale {
        static let button: CGFloat = 0.97
        static let island: CGFloat = 0.98
        static let popoverFrom: CGFloat = 0.96
    }

    /// Pressed: sinks fast.
    static let press = Animation.easeOut(duration: Duration.press)
    /// Released: comes back on the spring.
    static let release = Animation.spring(duration: Duration.release, bounce: 0)
    static let control = Animation.spring(duration: Duration.control, bounce: 0)
    static let popoverIn = Animation.spring(duration: Duration.popoverIn, bounce: 0)
    static let popoverOut = Animation.easeOut(duration: Duration.popoverOut)

    /// `--reduce-motion`: the same as the system setting, for checking without touching it.
    static let forcedReduce = CommandLine.arguments.contains("--reduce-motion")

    static func animation(_ animation: Animation, reduceMotion: Bool) -> Animation? {
        reduceMotion ? nil : animation
    }
}

extension EnvironmentValues {
    /// Reduce Motion from the system or from `--reduce-motion`.
    var reducesMotion: Bool { accessibilityReduceMotion || Motion.forcedReduce }
}

/// Buttons sink to 0.97 when pressed and come back on the spring.
struct PressScale: ViewModifier {
    let pressed: Bool
    var scale: CGFloat = Motion.Scale.button
    var anchor: UnitPoint = .center
    @Environment(\.self) private var env

    func body(content: Content) -> some View {
        content
            .scaleEffect(pressed ? scale : 1, anchor: anchor)
            .animation(Motion.animation(pressed ? Motion.press : Motion.release, reduceMotion: env.reducesMotion), value: pressed)
    }
}

/// The island takes clicks itself (it holds a button in the peak), with the pointing hand: sinks to 0.98 from the top centre while pressed.
/// A tap toggles the popover; past 3 pt the press becomes a drag that moves the panel and does not open anything.
struct PressableIsland: ViewModifier {
    static let dragThreshold: CGFloat = 3

    let model: IslandModel
    let tap: () -> Void
    @GestureState private var pressed = false
    @State private var dragging = false
    @State private var tracking = false

    func body(content: Content) -> some View {
        content
            .modifier(PressScale(pressed: pressed && !dragging, scale: Motion.Scale.island, anchor: .top))
            .pointingHand()
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .global)
                    .updating($pressed) { _, state, _ in state = true }
                    .onChanged { value in
                        if !tracking {
                            tracking = true
                            model.mover?.dragBegan()
                        }
                        if dragging || hypot(value.translation.width, value.translation.height) >= Self.dragThreshold {
                            dragging = true
                            model.mover?.dragChanged()
                        }
                    }
                    .onEnded { _ in
                        if dragging { model.mover?.dragEnded() } else { tap() }
                        dragging = false
                        tracking = false
                    }
            )
    }
}
