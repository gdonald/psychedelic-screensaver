// Loads the built bundle the way macOS does, then drives a few frames.
// Failures here are the ones that would otherwise show up as a black screen
// in System Settings.

import AppKit
import ScreenSaver

let arguments = CommandLine.arguments
guard arguments.count > 1, let bundle = Bundle(path: arguments[1]) else {
    FileHandle.standardError.write("usage: check-bundle <path to .saver>\n".data(using: .utf8)!)
    exit(1)
}

guard bundle.load() else {
    print("failed: the bundle would not load")
    exit(1)
}

guard let viewClass = bundle.principalClass as? ScreenSaverView.Type else {
    print("failed: the principal class is not a ScreenSaverView")
    exit(1)
}

let application = NSApplication.shared
application.setActivationPolicy(.accessory)

let frame = NSRect(x: 0, y: 0, width: 640, height: 400)
guard let view = viewClass.init(frame: frame, isPreview: false) else {
    print("failed: the view would not initialize")
    exit(1)
}

let window = NSWindow(
    contentRect: frame,
    styleMask: [.titled],
    backing: .buffered,
    defer: false
)
window.contentView = view

view.startAnimation()
for _ in 0..<30 {
    view.animateOneFrame()
    RunLoop.current.run(until: Date().addingTimeInterval(0.016))
}
view.stopAnimation()

let framesPresented = (view.value(forKey: "framesPresented") as? Int) ?? 0
guard framesPresented > 0 else {
    print("failed: the layer never handed out a drawable, so nothing was shown")
    exit(1)
}

guard view.hasConfigureSheet, view.configureSheet != nil else {
    print("failed: no configure sheet")
    exit(1)
}

print("ok: \(String(describing: viewClass)) loaded, presented \(framesPresented) frames and configured")
