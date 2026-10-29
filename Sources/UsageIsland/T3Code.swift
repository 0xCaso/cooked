import AppKit

/// T3 Code, the app the sources run in. `t3code://threads/<environment>/<thread>` opens one thread
/// (T3 Code 0.0.46 and later, pingdotgg/t3code#18454); older builds only come to the front.
enum T3Code {
    static let bundleID = "com.t3tools.t3code"
    /// The local server's environment id, the first part of a thread's route.
    static let environmentFile = NSHomeDirectory() + "/.t3/userdata/environment-id"

    static func link(thread: String, environmentFile: String = environmentFile) -> URL? {
        guard let env = try? String(contentsOfFile: environmentFile, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines), !env.isEmpty else { return nil }
        return URL(string: "t3code://threads/\(env)/\(thread)")
    }

    /// Opens the thread in the T3 Code that is running: Alpha, Nightly and custom builds share the bundle id and
    /// the scheme, so the link goes to that app, not to whichever one macOS picks. No thread: only to the front.
    @MainActor static func open(thread: String?) {
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).first
        let app = running?.bundleURL ?? NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID)
        guard let app else { return }
        if let thread, let url = link(thread: thread) {
            NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
        } else if let running {
            running.activate()
        } else {
            NSWorkspace.shared.openApplication(at: app, configuration: .init())
        }
    }
}
