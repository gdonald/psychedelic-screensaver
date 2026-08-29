import Foundation
import QuartzCore
import ScreenSaver

@objc(PsychedelicSaverView)
public final class PsychedelicSaverView: ScreenSaverView {
    private var saver: OpaquePointer?
    private let metalLayer = CAMetalLayer()
    private var lastFrame = CACurrentMediaTime()
    private var configureController: ConfigureSheetController?

    /// Frames the layer accepted a drawable for. Read by the bundle check to
    /// tell a running saver from one that is only being asked to draw.
    @objc public var framesPresented: Int {
        guard let saver else { return 0 }
        return Int(psy_frames_presented(saver))
    }

    public override init?(frame: NSRect, isPreview: Bool) {
        super.init(frame: frame, isPreview: isPreview)
        // A layer-hosting view takes its layer before wantsLayer is set. The
        // other order leaves AppKit managing a layer of its own.
        layer = metalLayer
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        animationTimeInterval = 1.0 / 60.0
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        layer = metalLayer
        wantsLayer = true
        animationTimeInterval = 1.0 / 60.0
    }

    deinit {
        psy_destroy(saver)
    }

    public override func startAnimation() {
        super.startAnimation()
        if saver == nil {
            saver = psy_create(Unmanaged.passUnretained(metalLayer).toOpaque(), UInt64.random(in: 0...UInt64.max))
            applySettings()
        }
        updateDrawableSize()
        lastFrame = CACurrentMediaTime()
    }

    public override func stopAnimation() {
        super.stopAnimation()
    }

    public override func animateOneFrame() {
        guard let saver else { return }
        let now = CACurrentMediaTime()
        let delta = min(now - lastFrame, 0.1)
        lastFrame = now
        psy_frame(saver, Float(delta))
    }

    public override func resize(withOldSuperviewSize oldSize: NSSize) {
        super.resize(withOldSuperviewSize: oldSize)
        updateDrawableSize()
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        updateDrawableSize()
    }

    private func updateDrawableSize() {
        guard let saver else { return }
        let scale = window?.backingScaleFactor ?? 2.0
        metalLayer.frame = bounds
        metalLayer.contentsScale = scale
        psy_resize(saver, Double(bounds.width * scale), Double(bounds.height * scale))
    }

    private func applySettings() {
        guard let saver else { return }
        let settings = Settings.load()
        psy_set_speed(saver, settings.speed)
        psy_set_scene_seconds(saver, settings.sceneSeconds)
        psy_set_mutation_strength(saver, settings.mutationStrength)
    }

    public override var hasConfigureSheet: Bool { true }

    public override var configureSheet: NSWindow? {
        let controller = ConfigureSheetController { [weak self] in
            self?.applySettings()
        }
        configureController = controller
        return controller.window
    }
}
