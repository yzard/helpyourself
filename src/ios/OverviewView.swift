import SwiftUI
import Charts

struct OverviewView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var snapshot: JSONValue = .null
    @State private var preferences: JSONValue = .null
    @State private var cached = false
    @State private var loading = false
    @State private var failure: String?
    @State private var customizing = false

    private var dateKey: String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = .current
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
    private var requestKey: String { dateKey + TimeZone.current.identifier }
    private var sourcePriorities: JSONValue {
        preferences["preferences"]["source_priority"] == .null ? .object([:]) : preferences["preferences"]["source_priority"]
    }
    private var visibleMetrics: [JSONValue] {
        let favorites = preferences["preferences"]["favorite_metrics"].arrayValue.map(\.stringValue)
        return snapshot["metrics"].arrayValue.filter { favorites.isEmpty || favorites.contains($0["record_type"].stringValue) }
    }

    var body: some View {
        List {
            Section {
                NavigationLink("Record a daily log", destination: DailyLogsView(model: model))
                NavigationLink("Sleep sessions", destination: SleepSessionsView(model: model))
                NavigationLink("Device trends", destination: DeviceTrendsView(model: model))
                NavigationLink("Timeline", destination: ArchiveTimelineView(model: model))
                NavigationLink("Breathing practice", destination: BreathingPracticeView(model: model))
                Button("Customize overview") { customizing = true }
                DatePicker("Day", selection: $date, displayedComponents: .date)
                Text(TimeZone.current.identifier).font(.caption).foregroundStyle(.secondary)
                if loading { ProgressView("Loading this day") }
                if let failure { Text(failure).foregroundStyle(.secondary) }
                if cached { Label("Cached data · refresh when connected", systemImage: "wifi.slash").font(.caption) }
                if let time = snapshot["computed_at"].numberValue {
                    Text("Calculated \(Date(timeIntervalSince1970: time).formatted())").font(.caption)
                }
            }
            ForEach(Array(visibleMetrics.enumerated()), id: \.offset) { _, metric in
                Section(metricTitle(metric["record_type"].stringValue)) {
                    NavigationLink {
                        DailyMetricView(metric: metric)
                    } label: {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(metricValue(metric["current"]["selected"]))
                                .font(.title2).monospacedDigit()
                            Text(metric["current"]["state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                            if metric["current"]["selected"] != .null {
                                Text(metric["current"]["selected"]["source"].stringValue).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                    let kind = metric["record_type"].stringValue
                    if kind == "sleep", let seconds = metric["current"]["selected"]["value"].numberValue,
                       let target = preferences["preferences"]["sleep_target_minutes"].numberValue {
                        Text("\((seconds / 60 - target).formatted()) minutes relative to your own daily target").font(.caption)
                    }
                    Picker("Preferred source", selection: Binding(get: { sourcePriorities[kind].arrayValue.first?.stringValue ?? "" }, set: { source in
                        Task { await saveAndReload(kind: kind, source: source) }
                    })) {
                        Text("Only when one source is available").tag("")
                        ForEach(metricSources(metric), id: \.self) { Text($0).tag($0) }
                    }
                    .disabled(loading)
                    if metric["baseline"] == .null {
                        Text("Personal comparison: \(Int(metric["baseline_valid_days"].numberValue ?? 0))/28 prior days").font(.caption)
                    } else {
                        Text("Prior median: \(metric["baseline"]["median"].numberValue?.formatted() ?? "") \(metric["unit"].stringValue)").font(.caption)
                    }
                }
            }
            Section {
                NavigationLink("Connect devices and view data", destination: DataCenterView(model: model))
                Text("Missing records are not zero. Calendar-day sleep is not a main sleep session. These summaries describe your archive.").font(.caption).foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Overview")
        .toolbar { NavigationLink(destination: SettingsView(model: model)) { Label("Settings", systemImage: "gearshape") } }
        .sheet(isPresented: $customizing) { NavigationStack { OverviewPreferencesView(model: model, preferences: $preferences) } }
        .task(id: requestKey) { await reload() }
        .refreshable { await reload() }
    }

    private func saveAndReload(kind: String, source: String) async {
        guard let client = model.client, let archive = model.archive, let token = model.session?.token,
              preferences["version"].numberValue != nil else { return }
        loading = true
        var updated = preferences["preferences"]
        updated["source_priority"][kind] = source.isEmpty ? .array([]) : .array([.string(source)])
        do {
            let result = try await model.request("wellness/preferences/save", body: .object([
                "expected_version": preferences["version"], "batch_id": .string(UUID().uuidString), "preferences": updated
            ]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            preferences = result
            try archive.save(result, name: "wellness-preferences.json")
            await reload()
        } catch {
            guard !Task.isCancelled, model.session?.token == token else { return }
            failure = error.localizedDescription
            loading = false
        }
    }
    private func reload() async {
        guard let client = model.client, let archive = model.archive, let token = model.session?.token else { return }
        let key = requestKey
        loading = true
        snapshot = .null
        failure = nil
        cached = false
        defer { if key == requestKey { loading = false } }
        do {
            preferences = try archive.load(JSONValue.self, name: "wellness-preferences.json") ?? .null
            let saved = try await model.request("wellness/preferences/get", body: .object([:]))
            guard !Task.isCancelled, model.session?.token == token, key == requestKey else { return }
            preferences = saved
            try archive.save(saved, name: "wellness-preferences.json")
            let result = try await model.request("wellness/day", body: .object([
                "date": .string(dateKey), "timezone": .string(TimeZone.current.identifier), "source_priority": sourcePriorities
            ]))
            guard !Task.isCancelled, model.session?.token == token, key == requestKey else { return }
            snapshot = result
            try archive.save(result, name: "daily-overview.json")
        } catch {
            guard !Task.isCancelled, model.session?.token == token, key == requestKey else { return }
            if case APIError.status(401, _) = error { model.errorMessage = error.localizedDescription; return }
            failure = error.localizedDescription
            if let result = try? archive.load(JSONValue.self, name: "daily-overview.json"),
               result["date"].stringValue == dateKey, result["timezone"].stringValue == TimeZone.current.identifier,
               result["source_priority"] == sourcePriorities {
                snapshot = result
                cached = true
            }
        }
    }
}

private struct DailyMetricView: View {
    let metric: JSONValue
    var body: some View {
        List {
            Section("Selected source history") {
                Chart {
                    ForEach(Array(metric["history"].arrayValue.enumerated()), id: \.offset) { _, day in
                        if let value = day["selected"]["value"].numberValue {
                            PointMark(x: .value("Day", day["date"].stringValue), y: .value("Value", value))
                                .foregroundStyle(by: .value("Source", day["selected"]["source"].stringValue))
                        }
                    }
                }.frame(height: 220).accessibilityLabel("Daily values by selected source")
                Text("\(metric["unit"].stringValue) · Points do not imply continuous coverage.").font(.caption)
            }
            Section("Daily records") {
                ForEach(Array(metric["history"].arrayValue.reversed().enumerated()), id: \.offset) { _, day in
                    DisclosureGroup(day["date"].stringValue + " · " + metricValue(day["selected"])) {
                        ForEach(Array(day["alternatives"].arrayValue.enumerated()), id: \.offset) { _, source in
                            VStack(alignment: .leading) {
                                Text(metricValue(source))
                                Text(source["source"].stringValue).font(.caption)
                            }
                        }
                    }
                }
            }
            Section("Calculation") {
                Text(metric["algorithm_version"].stringValue)
                Text("The comparison uses 28 prior valid days from the selected source within 42 days. It describes your data and does not estimate disease risk.").font(.caption)
            }
        }.navigationTitle(metricTitle(metric["record_type"].stringValue))
    }
}

private func metricSources(_ metric: JSONValue) -> [String] {
    Array(Set(metric["history"].arrayValue.flatMap { $0["alternatives"].arrayValue.map { $0["source"].stringValue } })).sorted()
}
private func metricValue(_ selected: JSONValue) -> String {
    guard let value = selected["value"].numberValue else { return "Not available" }
    return "\(value.formatted()) \(selected["unit"].stringValue)"
}
private func metricTitle(_ kind: String) -> String {
    ["sleep": "Sleep", "workout": "Activity", "steps": "Steps", "resting_heart_rate": "Resting heart rate", "heart_rate": "Heart rate", "hrv_sdnn": "HRV SDNN"][kind] ?? kind
}

struct DataCenterView: View {
    var model: AppModel
    @State private var sources: [JSONValue] = []
    var body: some View {
        List {
            Section {
                NavigationLink("Apple Health and sync coverage", destination: HealthView(model: model))
                NavigationLink("Import GPX, TCX, or FIT activities", destination: TrainingImportsView(model: model))
                NavigationLink("Export your complete archive", destination: SettingsView(model: model))
            }
            Section("Archived sources") {
                if sources.isEmpty { Text("No visible records. Connect Apple Health to start archiving data.").foregroundStyle(.secondary) }
                ForEach(Array(sources.enumerated()), id: \.offset) { _, source in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(source["source_id"].stringValue)
                        Text("\(source["platform"].stringValue) · \(source["record_type"].stringValue)").font(.caption)
                        Text("\(Int(source["record_count"].numberValue ?? 0)) records").font(.caption)
                    }
                }
            }
        }.navigationTitle("Data")
            .toolbar { NavigationLink(destination: SettingsView(model: model)) { Label("Settings", systemImage: "gearshape") } }
            .task { await reload() }.refreshable { await reload() }
    }
    private func reload() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let result = try await model.request("wellness/sources", body: .object([:]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            sources = result["sources"].arrayValue
        } catch { if !Task.isCancelled, model.session?.token == token { model.errorMessage = error.localizedDescription } }
    }
}

private struct OverviewPreferencesView: View {
    var model: AppModel
    @Binding var preferences: JSONValue
    @Environment(\.dismiss) private var dismiss
    @State private var favorites: Set<String> = []
    @State private var target = ""
    @State private var snapshot: JSONValue = .null
    @State private var failure: String?
    @State private var loading = true
    private let kinds = ["sleep", "workout", "steps", "resting_heart_rate", "heart_rate", "hrv_sdnn"]
    var body: some View {
        Form {
            Section("Favorite metrics") {
                ForEach(kinds, id: \.self) { kind in
                    Toggle(metricTitle(kind), isOn: Binding(get: { favorites.contains(kind) }, set: { selected in
                        if selected { favorites.insert(kind) } else { favorites.remove(kind) }
                    }))
                }
                Text("No favorites shows all six metrics.").font(.caption)
            }
            Section("Your daily sleep target") {
                TextField("Minutes, optional", text: $target).keyboardType(.numberPad)
                Text("This is your own goal, not a calculated biological sleep need.").font(.caption)
            }
            if let failure { Section { Text(failure) } }
            Button("Save preferences") { Task { await save() } }.disabled(loading || snapshot == .null)
        }.disabled(loading).navigationTitle("Customize overview")
            .toolbar { Button("Cancel") { dismiss() } }
            .task { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { loading = false; return }
        do {
            let result = try await model.request("wellness/preferences/get", body: .object([:]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            snapshot = result
            favorites = Set(result["preferences"]["favorite_metrics"].arrayValue.map(\.stringValue))
            target = result["preferences"]["sleep_target_minutes"].numberValue.map { String(Int($0)) } ?? ""
            loading = false
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription; loading = false } }
    }
    private func save() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let text = target.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.isEmpty || Int(text).map({ (1...1440).contains($0) }) == true else { failure = "Use a whole number from 1 to 1440 minutes."; return }
        var updated = snapshot["preferences"]
        updated["favorite_metrics"] = .array(kinds.filter { favorites.contains($0) }.map(JSONValue.string))
        updated["sleep_target_minutes"] = text.isEmpty ? .null : .number(Double(text) ?? 0)
        loading = true
        do {
            let result = try await model.request("wellness/preferences/save", body: .object(["expected_version": snapshot["version"], "batch_id": .string(UUID().uuidString), "preferences": updated]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            preferences = result
            try model.archive?.save(result, name: "wellness-preferences.json")
            dismiss()
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription; loading = false } }
    }
}
