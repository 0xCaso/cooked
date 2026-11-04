import AppKit

/// A click on a row (release spec, section 1, Click on a row): the place the session runs in, else a Terminal
/// that resumes it.
@MainActor
enum SessionOpener {
    static let claudeBundleID = "com.anthropic.claudefordesktop"
    static let codexBundleID = "com.openai.codex"
    /// The `.command` files a Terminal fallback opens.
    static let commandsDir = NSHomeDirectory() + "/Library/Application Support/Cooked/open"

    /// What the lifted row says: where the click goes.
    static func label(_ target: OpenTarget?) -> String {
        guard let target else { return "Open T3 Code ↗" }
        switch target.place {
        case .t3Code: return "Open in T3 Code ↗"
        case .codex: return installed(codexBundleID) ? "Open in Codex ↗" : "Open in Terminal ↗"
        case .claude: return target.configDir == nil && installed(claudeBundleID) ? "Open in Claude ↗" : "Open in Terminal ↗"
        }
    }

    static func open(_ target: OpenTarget?) {
        guard let target else { return T3Code.open(thread: nil) }
        let link: (String, String)? = switch target.place {
        case .t3Code: nil
        case .codex: (codexBundleID, "codex://threads/\(target.id)")
        // The Claude app only finds `~/.claude` sessions; other accounts fail there without telling Cooked.
        case .claude: target.configDir == nil ? (claudeBundleID, "claude://resume?session=\(target.id)") : nil
        }
        if target.place == .t3Code {
            T3Code.open(thread: target.id)
        } else if let link, let app = app(link.0), let url = URL(string: link.1) {
            send(url, to: app, bundleID: link.0)
        } else {
            terminal(target)
        }
    }

    /// A link sent while the app is still loading fails ("Couldn't open that session", seen on 2026-11-04): a closed
    /// app opens first and gets the link a few seconds later.
    private static func send(_ url: URL, to app: URL, bundleID: String) {
        let open = { NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration()) }
        guard NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).isEmpty else { return open() }
        NSWorkspace.shared.openApplication(at: app, configuration: NSWorkspace.OpenConfiguration())
        DispatchQueue.main.asyncAfter(deadline: .now() + launchWait) { open() }
    }

    static let launchWait: TimeInterval = 5

    private static func app(_ bundleID: String) -> URL? { NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) }
    private static func installed(_ bundleID: String) -> Bool { app(bundleID) != nil }

    /// A new Terminal window resumes the session in its folder: a `.command` file needs no Automation permission.
    private static func terminal(_ target: OpenTarget) {
        let script = target.script
        let path = commandsDir + "/\(target.id.filter { $0.isLetter || $0.isNumber || $0 == "-" }).command"
        do {
            try FileManager.default.createDirectory(atPath: commandsDir, withIntermediateDirectories: true)
            try script.write(toFile: path, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
        } catch { return }
        guard let terminal = app("com.apple.Terminal") else { return }
        NSWorkspace.shared.open([URL(fileURLWithPath: path)], withApplicationAt: terminal, configuration: NSWorkspace.OpenConfiguration())
    }
}
