import SwiftUI

struct BehaviorAssociationsView: View {
    var model: AppModel
    @State private var outcome = "sleep"
    @State private var source = ""
    @State private var endDate = Calendar.current.date(byAdding: .day, value: -1, to: Date()) ?? Date()
    @State private var behaviors = ""
    @State private var covariates = ""
    @State private var lag = 1
    @State private var acknowledged = false
    @State private var sources: [JSONValue] = []
    @State private var saved: [JSONValue] = []
    @State private var result: JSONValue = .null
    @State private var failure: String?
    @State private var loading = false
    private let outcomes = ["sleep", "hrv_sdnn", "resting_heart_rate", "heart_rate", "workout", "steps"]
    private var choices: [String] { sources.filter { $0["record_type"].stringValue == outcome }.map { $0["platform"].stringValue + ":" + $0["source_id"].stringValue } }
    var body: some View {
        Form {
            Section("Declare an analysis") {
                Text("Exploratory statistics, not causal effects. Declare all comparisons before inspecting results. Repeated searches are not covered by the correction.").font(.caption)
                Picker("Outcome", selection: $outcome) { ForEach(outcomes, id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ")).tag($0) } }
                Picker("One source", selection: $source) {
                    Text("Select a source").tag("")
                    ForEach(choices, id: \.self) { Text($0).tag($0) }
                }
                DatePicker("Last completed date", selection: $endDate, in: ...Date(), displayedComponents: .date)
                TextField("Behavior names, one per line (up to 8)", text: $behaviors, axis: .vertical).lineLimit(2...8)
                TextField("Covariate names, one per line (up to 3)", text: $covariates, axis: .vertical).lineLimit(1...3)
                Picker("Declared lag", selection: $lag) { Text("Same date").tag(0); Text("Previous date").tag(1) }
                Toggle("I selected this family before viewing results and will check source protocol changes and missing data.", isOn: $acknowledged)
                Button("Run declared analysis") { Task { await run() } }.disabled(loading || !acknowledged || source.isEmpty || names(behaviors).isEmpty)
                if loading { ProgressView() }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            if result != .null {
                Section("Analysis scope") {
                    Text(result["parameters"]["source"].stringValue)
                    Text("End date: " + result["parameters"]["end_date"].stringValue)
                    Text("90 calendar days. HRV SDNN is not RMSSD. Sleep and workout durations use seconds.").font(.caption)
                }
                ForEach(Array(result["results"].arrayValue.enumerated()), id: \.offset) { _, row in
                    Section(row["behavior"].stringValue) {
                        Text(row["state"].stringValue.replacingOccurrences(of: "_", with: " "))
                        Text("\(Int(row["sample_days"].numberValue ?? 0))/90 complete days")
                        if row["estimate"] != .null {
                            let estimate = row["estimate"]
                            Text("Association: \(estimate["effect"].numberValue?.formatted() ?? "Unknown") \(row["outcome_unit"].stringValue) per \(row["predictor_unit"].stringValue)")
                            Text("Nominal 95% interval: " + bounds(estimate["ci_95"]))
                            Text("Nominal p: \(probability(estimate["p_value"])) · nominal BY q: \(probability(row["q_value"]))")
                            Text("Block bootstrap 95% interval: " + bounds(estimate["bootstrap_ci_95"]))
                            Text("Significance decisions are disabled: this protocol failed synthetic error-rate calibration. Values are exploratory diagnostics.").font(.caption)
                        }
                        DisclosureGroup("Daily inclusion and missingness") {
                            ForEach(Array(row["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                                Text(day["date"].stringValue + " · " + day["state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                            }
                        }
                    }
                }
                Section("Method") {
                    ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
                    if let data = try? JSONEncoder().encode(result), let text = String(data: data, encoding: .utf8) { ShareLink("Share result, protocol and inputs", item: text) }
                }
            }
            Section("Saved current analyses") {
                Text("Source edits remove these results and their input snapshots. Current results are included in the full archive export.").font(.caption)
                ForEach(Array(saved.enumerated()), id: \.offset) { _, item in
                    Button(item["parameters"]["outcome"].stringValue + " · " + item["parameters"]["end_date"].stringValue) { Task { await open(item["result_id"].stringValue) } }.disabled(loading)
                }
            }
        }.navigationTitle("Behavior associations")
            .task { await load() }
            .onChange(of: outcome) { _, _ in source = choices.first ?? ""; acknowledged = false }
            .onChange(of: behaviors) { _, _ in acknowledged = false }
            .onChange(of: covariates) { _, _ in acknowledged = false }
    }
    private func names(_ text: String) -> [String] { text.components(separatedBy: .newlines).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty } }
    private func bounds(_ value: JSONValue) -> String { value == .null ? "Not available" : value.arrayValue.compactMap(\.numberValue).map { $0.formatted() }.joined(separator: " to ") }
    private func probability(_ value: JSONValue) -> String { value.numberValue.map { String(format: "%.4g", $0) } ?? "Unknown" }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let catalog = try await model.request("wellness/sources", body: .object([:]))
            let history = try await model.request("wellness/associations/list", body: .object([:]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            sources = catalog["sources"].arrayValue; saved = history["results"].arrayValue
            if source.isEmpty { source = choices.first ?? "" }
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
    private func run() async {
        let formatter = DateFormatter(); formatter.calendar = Calendar(identifier: .gregorian); formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = .current; formatter.dateFormat = "yyyy-MM-dd"
        await request("wellness/associations/run", body: .object(["end_date": .string(formatter.string(from: endDate)), "timezone": .string(TimeZone.current.identifier), "outcome": .string(outcome), "source": .string(source), "behaviors": .array(names(behaviors).map(JSONValue.string)), "covariates": .array(names(covariates).map(JSONValue.string)), "lag_days": .number(Double(lag))]))
        await load()
    }
    private func open(_ id: String) async { await request("wellness/associations/get", body: .object(["result_id": .string(id)])) }
    private func request(_ path: String, body: JSONValue) async {
        guard !loading, let client = model.client, let token = model.session?.token else { return }
        loading = true; defer { loading = false }
        do {
            let response = try await model.request(path, body: body)
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
