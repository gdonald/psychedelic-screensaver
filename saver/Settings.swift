import Foundation
import ScreenSaver

/// Screen saver preferences, stored per module so the settings sheet and the
/// running saver agree.
struct Settings {
    static let moduleName = "com.gdonald.psychedelic"

    var speed: Float
    var sceneSeconds: Float
    var mutationStrength: Float

    static let defaults = Settings(speed: 1.0, sceneSeconds: 60.0, mutationStrength: 0.6)

    static func load() -> Settings {
        guard let store = ScreenSaverDefaults(forModuleWithName: moduleName) else {
            return defaults
        }
        store.register(defaults: [
            "speed": defaults.speed,
            "sceneSeconds": defaults.sceneSeconds,
            "mutationStrength": defaults.mutationStrength,
        ])
        return Settings(
            speed: store.float(forKey: "speed"),
            sceneSeconds: store.float(forKey: "sceneSeconds"),
            mutationStrength: store.float(forKey: "mutationStrength")
        )
    }

    func save() {
        guard let store = ScreenSaverDefaults(forModuleWithName: Settings.moduleName) else {
            return
        }
        store.set(speed, forKey: "speed")
        store.set(sceneSeconds, forKey: "sceneSeconds")
        store.set(mutationStrength, forKey: "mutationStrength")
        store.synchronize()
    }
}
