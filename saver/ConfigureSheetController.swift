import AppKit

/// The settings sheet, built in code so the bundle needs no compiled nib.
final class ConfigureSheetController: NSObject {
    let window: NSWindow
    private let onApply: () -> Void
    private var settings = Settings.load()

    private let speedSlider = NSSlider()
    private let sceneSlider = NSSlider()
    private let mutationSlider = NSSlider()
    private let speedValue = NSTextField(labelWithString: "")
    private let sceneValue = NSTextField(labelWithString: "")
    private let mutationValue = NSTextField(labelWithString: "")

    init(onApply: @escaping () -> Void) {
        self.onApply = onApply
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 210),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        super.init()
        window.title = "Psychedelic"
        buildContent()
        refreshLabels()
    }

    private func buildContent() {
        let rows = NSStackView(views: [
            row(label: "Drift speed", slider: speedSlider, value: speedValue,
                range: 0.1...4.0, current: Double(settings.speed), action: #selector(speedChanged)),
            row(label: "Seconds per pattern", slider: sceneSlider, value: sceneValue,
                range: 5.0...180.0, current: Double(settings.sceneSeconds), action: #selector(sceneChanged)),
            row(label: "Mutation", slider: mutationSlider, value: mutationValue,
                range: 0.0...1.0, current: Double(settings.mutationStrength), action: #selector(mutationChanged)),
        ])
        rows.orientation = .vertical
        rows.alignment = .leading
        rows.spacing = 14

        let done = NSButton(title: "Done", target: self, action: #selector(done))
        done.keyEquivalent = "\r"
        let buttons = NSStackView(views: [done])
        buttons.orientation = .horizontal
        buttons.alignment = .centerY

        let content = NSStackView(views: [rows, buttons])
        content.orientation = .vertical
        content.alignment = .trailing
        content.spacing = 20
        content.edgeInsets = NSEdgeInsets(top: 20, left: 20, bottom: 20, right: 20)
        content.translatesAutoresizingMaskIntoConstraints = false
        window.contentView = content
    }

    private func row(
        label: String,
        slider: NSSlider,
        value: NSTextField,
        range: ClosedRange<Double>,
        current: Double,
        action: Selector
    ) -> NSView {
        let title = NSTextField(labelWithString: label)
        title.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        slider.minValue = range.lowerBound
        slider.maxValue = range.upperBound
        slider.doubleValue = current
        slider.target = self
        slider.action = action
        slider.isContinuous = true
        slider.widthAnchor.constraint(equalToConstant: 160).isActive = true
        value.widthAnchor.constraint(equalToConstant: 48).isActive = true
        title.widthAnchor.constraint(equalToConstant: 150).isActive = true

        let stack = NSStackView(views: [title, slider, value])
        stack.orientation = .horizontal
        stack.spacing = 10
        return stack
    }

    private func refreshLabels() {
        speedValue.stringValue = String(format: "%.2fx", settings.speed)
        sceneValue.stringValue = String(format: "%.0fs", settings.sceneSeconds)
        mutationValue.stringValue = String(format: "%.2f", settings.mutationStrength)
    }

    @objc private func speedChanged() {
        settings.speed = Float(speedSlider.doubleValue)
        refreshLabels()
    }

    @objc private func sceneChanged() {
        settings.sceneSeconds = Float(sceneSlider.doubleValue)
        refreshLabels()
    }

    @objc private func mutationChanged() {
        settings.mutationStrength = Float(mutationSlider.doubleValue)
        refreshLabels()
    }

    @objc private func done() {
        settings.save()
        onApply()
        window.sheetParent?.endSheet(window)
    }
}
