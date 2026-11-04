import SwiftUI

/// The window's width; the height follows the content.
enum SetupLayout {
    static let width: CGFloat = 600
    static let dark = Color(red: 17 / 255, green: 17 / 255, blue: 17 / 255)
}

/// "today 11:50", "yesterday", "Sat 31 Oct"; "never" for a folder with no sessions.
enum LastUse {
    static func text(_ date: Date?, now: Date = Date(), calendar: Calendar = .current) -> String {
        guard let date else { return "never" }
        if calendar.isDate(date, inSameDayAs: now) { return "today " + format("HH:mm", date, calendar) }
        if let y = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: y) { return "yesterday" }
        return format("EEE d MMM", date, calendar)
    }

    private static func format(_ pattern: String, _ date: Date, _ calendar: Calendar) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_GB")
        f.calendar = calendar
        f.timeZone = calendar.timeZone
        f.dateFormat = pattern
        return f.string(from: date)
    }
}

/// The found folders: tick, name, folder, last use. Setup step 1 and the Accounts tab.
struct AccountsList: View {
    @Bindable var model: SetupModel
    var onChange: () -> Void = {}

    var body: some View {
        VStack(spacing: 0) {
            ForEach($model.rows) { $row in
                HStack(spacing: 12) {
                    SetupCheckbox(isOn: $row.ticked, label: "Read \(row.found.path)")
                    NameField(text: $row.name, enabled: row.ticked, onCommit: onChange).frame(width: 150)
                    Text(model.tilde(row.found.path))
                        .font(TextStyle.labelNumber.font)
                        .foregroundStyle(Palette.ink3)
                        .lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(LastUse.text(row.found.lastUsed))
                        .font(TextStyle.labelNumber.font)
                        .foregroundStyle(row.ticked ? Palette.ink2 : Palette.ink3)
                        .lineLimit(1)
                        .frame(width: 96, alignment: .trailing)
                }
                .frame(minHeight: 40)
                .overlay(alignment: .top) { if row.id != model.rows.first?.id { Rectangle().fill(Palette.line).frame(height: 0.5) } }
                .onChange(of: row.ticked) { onChange() }
            }
            if model.rows.isEmpty {
                Text("No Claude Code or Codex folder found in your home folder.")
                    .textStyle(.body).foregroundStyle(Palette.ink2).frame(minHeight: 40)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .cardSurface()
    }
}

// MARK: - Setup

/// First launch: three steps, each one optional. Accounts, the Claude percent (status line), open at login.
struct SetupView: View {
    @Bindable var model: SetupModel
    var onDone: () -> Void = {}
    @State private var forward = true
    @Environment(\.self) private var env

    var body: some View {
        VStack(spacing: 0) {
            rail
            ZStack(alignment: .topLeading) {
                stepBody(model.step)
                    .id(model.step)
                    .transition(SetupMotion.step(forward: forward, reduceMotion: env.reducesMotion))
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .clipped()
            foot
        }
        .frame(width: SetupLayout.width)
        .background(Palette.bgDeep)
    }

    private var rail: some View {
        let steps = model.steps
        return HStack(spacing: 10) {
            ForEach(Array(steps.enumerated()), id: \.element) { i, step in
                if i > 0 { Rectangle().fill(Palette.ink.opacity(0.14)).frame(height: 1) }
                railStep(step, number: i + 1)
            }
        }
        .padding(EdgeInsets(top: 4, leading: 24, bottom: 14, trailing: 24))
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.line).frame(height: 0.5) }
    }

    private func railStep(_ step: SetupModel.Step, number: Int) -> some View {
        let current = step == model.step, done = step < model.step
        return HStack(spacing: 8) {
            ZStack {
                if current {
                    Circle().fill(Palette.ink)
                } else {
                    Circle().strokeBorder(Palette.ink.opacity(done ? 0.55 : 0.3), lineWidth: 1)
                }
                if done {
                    Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).foregroundStyle(Palette.ink)
                } else {
                    Text("\(number)").font(TextStyle.numberEmphasis.font).font(.system(size: 12))
                        .foregroundStyle(current ? Color(white: 0.07) : Palette.ink3)
                }
            }
            .frame(width: 20, height: 20)
            Text(step.title)
                .font((current ? TextStyle.emphasis : TextStyle.body).font)
                .foregroundStyle(current ? Palette.ink : done ? Palette.ink2 : Palette.ink3)
                .fixedSize()
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(current ? "current step" : done ? "done" : "")
    }

    @ViewBuilder
    private func stepBody(_ step: SetupModel.Step) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            switch step {
            case .accounts:
                title("Pick your accounts")
                lead("The name is what the menu bar shows.")
                AccountsList(model: model)
            case .percent:
                title("Add Cooked to your status line?")
                lead("It gives the Claude percent in the terminal. T3 Code and Codex already show it.")
                SetupDisclosure("What changes") { whatChanges }
                problemLine
            case .login:
                HStack(spacing: 16) {
                    title("Open Cooked at login?").frame(maxWidth: .infinity, alignment: .leading)
                    SetupSwitch(isOn: $model.openAtLogin, label: "Open Cooked at login")
                }
            }
        }
        .padding(EdgeInsets(top: 22, leading: 24, bottom: 20, trailing: 24))
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private var whatChanges: some View {
        VStack(alignment: .leading, spacing: 10) {
            FactList(facts: [
                Text("Only the \(Text("statusLine").styled(.labelNumber)) key changes. A backup is made once."),
                Text("Your status line looks the same. Disconnect puts back the exact old text."),
                Text("Skip: the terminal shows tokens per minute, no percent."),
            ])
            SettingsFilesTree(files: model.settingsFiles)
        }
    }

    @ViewBuilder
    private var problemLine: some View {
        if let problem = model.problem {
            Text(problem).textStyle(.label).foregroundStyle(Palette.ink2)
        }
    }

    private var foot: some View {
        HStack(spacing: 8) {
            if model.step != .login {
                Button("Skip") { go(skip: true) }.buttonStyle(GhostButtonStyle())
            }
            Spacer(minLength: 0)
            if let back = model.back(from: model.step) {
                Button("Back") { move(to: back, forward: false) }.buttonStyle(SecondaryButtonStyle())
            }
            Button(primaryTitle) { go(skip: false) }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
        .padding(EdgeInsets(top: 12, leading: 16, bottom: 14, trailing: 16))
        .overlay(alignment: .top) { Rectangle().fill(Palette.line).frame(height: 0.5) }
    }

    private var primaryTitle: String {
        switch model.step {
        case .accounts: "Continue"
        case .percent: model.percent == .connected ? "Continue" : "Allow"
        case .login: "Done"
        }
    }

    /// The primary button acts, then moves on; Skip only moves on (Skip on Accounts keeps every folder ticked).
    private func go(skip: Bool) {
        switch model.step {
        case .accounts: model.saveAccounts()
        case .percent:
            if !skip, model.percent != .connected {
                model.connect()
                if model.problem != nil { return }
            }
        case .login:
            model.finish()
            onDone()
            return
        }
        if let next = model.next(after: model.step) { move(to: next, forward: true) }
    }

    private func move(to step: SetupModel.Step, forward: Bool) {
        self.forward = forward
        SetupMotion.run { model.step = step }
    }

    private func title(_ s: String) -> some View {
        Text(s).font(.custom(PlexFont.sansMedium, fixedSize: 17)).foregroundStyle(Palette.ink)
            .frame(minHeight: 22).accessibilityAddTraits(.isHeader)
    }

    private func lead(_ s: String) -> some View {
        Text(s).font(TextStyle.body.font).lineSpacing(3).foregroundStyle(Palette.ink2).padding(.bottom, 10)
            .fixedSize(horizontal: false, vertical: true)
    }
}

// MARK: - Settings

/// The same three groups as Setup, as tabs, plus "Remove Cooked's changes".
struct SettingsView: View {
    enum Tab: String, CaseIterable, Identifiable {
        case accounts = "Accounts", percent = "Claude percent", general = "General"
        var id: Self { self }
        var symbol: String {
            switch self {
            case .accounts: "person.2"
            case .percent: "percent"
            case .general: "power"
            }
        }
    }

    @Bindable var model: SetupModel
    @State var tab: Tab = .accounts
    @State var confirmingRemove = false
    @State private var forward = true
    @Environment(\.self) private var env

    var body: some View {
        VStack(spacing: 0) {
            tabs
            ZStack(alignment: .topLeading) {
                content(tab)
                    .id(tab)
                    .transition(SetupMotion.step(forward: forward, reduceMotion: env.reducesMotion))
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .clipped()
        }
        .frame(width: SetupLayout.width)
        .background(Palette.bgDeep)
        .overlay(alignment: .top) { removeSheet }
        .onAppear { model.refresh() }
    }

    private var tabs: some View {
        HStack(spacing: 4) {
            ForEach(Tab.allCases) { t in
                Button {
                    forward = Tab.allCases.firstIndex(of: t)! > Tab.allCases.firstIndex(of: tab)!
                    SetupMotion.run { tab = t }
                } label: {
                    VStack(spacing: 4) {
                        Image(systemName: t.symbol).font(.system(size: 15)).frame(height: 18)
                        Text(t.rawValue).textStyle(.label)
                    }
                    .foregroundStyle(t == tab ? Palette.ink : Palette.ink2)
                    .padding(.horizontal, 14).padding(.vertical, 6)
                    .background {
                        if t == tab {
                            let shape = RoundedRectangle(cornerRadius: 10, style: .continuous)
                            shape.fill(Color(white: 0.11))
                            shape.inset(by: -0.25).stroke(.white.opacity(0.12), lineWidth: 0.5)
                        }
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .pointingHand()
                .accessibilityAddTraits(t == tab ? .isSelected : [])
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.bottom, 12)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.line).frame(height: 0.5) }
    }

    @ViewBuilder
    private func content(_ tab: Tab) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            switch tab {
            case .accounts:
                AccountsList(model: model) { model.saveAccounts() }
                Text("Ticked accounts show in the menu bar. The name is what it shows.")
                    .textStyle(.label).foregroundStyle(Palette.ink3)
            case .percent:
                percent
            case .general:
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Open Cooked at login").textStyle(.emphasis).foregroundStyle(Palette.ink)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    SetupSwitch(isOn: Binding(get: { model.openAtLogin }, set: { model.setOpenAtLogin($0) }),
                                label: "Open Cooked at login")
                }
                .padding(12)
                .cardSurface()
                HStack {
                    Button("Remove Cooked's changes…") { show(true) }.buttonStyle(GhostButtonStyle())
                    Spacer(minLength: 0)
                }
                .padding(.leading, -10)
            }
            if let problem = model.problem {
                Text(problem).textStyle(.label).foregroundStyle(Palette.ink2)
            }
        }
        .padding(EdgeInsets(top: 18, leading: 24, bottom: 20, trailing: 24))
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    @ViewBuilder
    private var percent: some View {
        if model.settingsFiles.isEmpty {
            status(glyph: "minus", filled: false, title: "No Claude account", line: "Tick a Claude account in Accounts first.") { EmptyView() }
        } else {
            switch model.percent {
            case .connected:
                status(glyph: "checkmark", filled: true, title: "Connected", line: "Your status line looks the same.") {
                    Button("Disconnect") { model.disconnect() }.buttonStyle(SecondaryButtonStyle())
                }
                SetupDisclosure("Details") {
                    VStack(alignment: .leading, spacing: 10) {
                        FactList(facts: connectedFacts)
                        SettingsFilesTree(files: model.settingsFiles)
                    }
                }
            case .notConnected:
                status(glyph: "minus", filled: false, title: "Not connected", line: "The terminal shows tokens per minute only.") {
                    Button("Connect") { model.connect() }.buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction)
                }
                SetupDisclosure("What changes") {
                    VStack(alignment: .leading, spacing: 10) {
                        FactList(facts: [
                            Text("Only the \(Text("statusLine").styled(.labelNumber)) key changes. A backup is made once."),
                            Text("Your status line looks the same. Disconnect puts back the exact old text."),
                        ])
                        SettingsFilesTree(files: model.settingsFiles)
                    }
                }
            case .changed:
                status(glyph: "exclamationmark.triangle", filled: false, title: "Your status line changed. Connect again?",
                       line: "The terminal percent is paused.") {
                    Button("Connect again") { model.connect() }.buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction)
                }
                SetupDisclosure("Details") {
                    FactList(facts: changedFacts)
                }
            }
        }
    }

    private var connectedFacts: [Text] {
        var facts: [Text] = []
        if let last = model.lastTerminalPercent() {
            let time = Text(last.at.formatted(.dateTime.hour(.twoDigits(amPM: .omitted)).minute())).styled(.labelNumber)
            facts.append(Text("Last terminal percent: \(last.name) \(Text("\(last.percent)%").styled(.labelNumber)) at \(time)."))
        } else {
            facts.append(Text("No terminal percent yet: it comes after your next message in claude."))
        }
        if let command = model.wrappedCommand() {
            facts.append(Text("Your status line still runs \(Text(command).styled(.labelNumber).foregroundColor(Palette.ink))."))
        }
        return facts
    }

    private var changedFacts: [Text] {
        var facts = [Text("Something replaced Cooked's command. Cooked did not touch it. Connect again keeps the new command and adds Cooked around it.")]
        if let now = model.currentCommand() {
            facts.append(Text("Now it runs \(Text(now).styled(.labelNumber).foregroundColor(Palette.ink))."))
        }
        return facts
    }

    private func status<Action: View>(glyph: String, filled: Bool, title: String, line: String,
                                      @ViewBuilder action: () -> Action) -> some View {
        HStack(spacing: 12) {
            ZStack {
                if filled { Circle().fill(Palette.ink) } else { Circle().strokeBorder(Palette.ink.opacity(0.35), lineWidth: 1) }
                Image(systemName: glyph).font(.system(size: 10, weight: .bold))
                    .foregroundStyle(filled ? Color(white: 0.07) : Palette.ink2)
            }
            .frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 0) {
                Text(title).textStyle(.emphasis).foregroundStyle(Palette.ink)
                Text(line).textStyle(.body).foregroundStyle(Palette.ink2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            action()
        }
        .accessibilityElement(children: .contain)
    }

    // MARK: Remove

    private func show(_ on: Bool) {
        withAnimation(Motion.animation(on ? SetupMotion.enter : SetupMotion.leave, reduceMotion: false)) { confirmingRemove = on }
    }

    @ViewBuilder
    private var removeSheet: some View {
        if confirmingRemove {
            ZStack(alignment: .top) {
                Color.black.opacity(0.45)
                    .contentShape(.rect)
                    .onTapGesture { show(false) }
                    .transition(.opacity)
                VStack(alignment: .leading, spacing: 0) {
                    Text("Remove Cooked's changes?").font(.custom(PlexFont.sansMedium, fixedSize: 15)).foregroundStyle(Palette.ink)
                        .padding(.bottom, 10)
                    Text(model.removeSentence).font(TextStyle.body.font).lineSpacing(3).foregroundStyle(Palette.ink2)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 8) {
                        Spacer(minLength: 0)
                        Button("Cancel") { show(false) }.buttonStyle(SecondaryButtonStyle()).keyboardShortcut(.cancelAction)
                        Button("Remove changes") {
                            model.removeAll()
                            show(false)
                        }
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                    }
                    .padding(.top, 12)
                }
                .padding(20)
                .frame(width: 440)
                .background {
                    let shape = UnevenRoundedRectangle(bottomLeadingRadius: 18, bottomTrailingRadius: 18, style: .continuous)
                    shape.fill(Palette.bgBase).shadow(color: .black.opacity(0.6), radius: 20, y: 20)
                    shape.inset(by: -0.25).stroke(.white.opacity(0.16), lineWidth: 0.5)
                }
                .transition(env.reducesMotion ? .opacity : .offset(y: -SetupMotion.drop).combined(with: .opacity))
                .accessibilityAddTraits(.isModal)
            }
        }
    }
}
