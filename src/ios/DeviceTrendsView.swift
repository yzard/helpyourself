import SwiftUI
import Charts

struct DeviceTrendsView: View {
    var model: AppModel
    @State private var kind = "blood_glucose"
    @State private var result: JSONValue = .null
    @State private var failure: String?
    @State private var loading = false
    @State private var maximumBPM = ""
    @State private var maximumSource = ""
    @State private var maximumGap = "15"
    private let kinds = [("heart_rate", "Heart rate"), ("blood_glucose", "Blood glucose"), ("vo2_max", "VO₂ Max"), ("body_mass", "Body weight"), ("body_fat", "Body fat"), ("oxygen_saturation", "Oxygen saturation"), ("respiratory_rate", "Respiratory rate")]
    var body: some View {
        List {
            Section {
                Picker("Measurement", selection: $kind) {
                    ForEach(kinds, id: \.0) { Text($0.1).tag($0.0) }
                }
                Text(kind == "heart_rate" ? "Last 24 hours. Sources remain separate." : "Last 30 days. Sources remain separate.").font(.caption)
                if kind == "heart_rate" {
                    TextField("Maximum sample gap (seconds)", text: $maximumGap).keyboardType(.numberPad)
                    TextField("Declared maximum (optional, bpm)", text: $maximumBPM).keyboardType(.decimalPad)
                    TextField("Source of declared maximum", text: $maximumSource)
                    Text("The initial 15-second gap is an engineering assumption. Match your source protocol.").font(.caption)
                    Button("Apply sampling protocol") { Task { await reload() } }.disabled(loading)
                }
                if loading { ProgressView() }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(Array(result["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                Section(source["source"].stringValue) {
                    Text(source["state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                    Chart {
                        ForEach(Array(source["points"].arrayValue.enumerated().filter { $0.offset % max(1, source["points"].arrayValue.count / 1000) == 0 }), id: \.offset) { _, point in
                            if let value = point["value"].numberValue, let at = point["at"].numberValue {
                                PointMark(x: .value("Time", Date(timeIntervalSince1970: at)), y: .value(result["unit"].stringValue, value))
                            }
                        }
                    }.frame(height: 220).accessibilityLabel("Sampled observations in \(result["unit"].stringValue). Open records for exact values.")
                    Text("\(result["unit"].stringValue) · Points do not imply continuous coverage.").font(.caption)
                    if source["glucose_summary"] != .null {
                        let glucose = source["glucose_summary"]
                        Text("Observed coverage: \(((glucose["coverage_fraction"].numberValue ?? 0) * 100).formatted())%")
                        Text("Time in 70–180 mg/dL: \(glucose["tir_percent"].numberValue?.formatted() ?? "Not available")\(glucose["tir_percent"] == .null ? "" : "%")")
                        Text("Mean: \(glucose["mean_mg_dl"].numberValue?.formatted() ?? "Not available") mg/dL")
                    }
                    if source["heart_rate_summary"] != .null {
                        let heart = source["heart_rate_summary"]
                        Text(heart["state"].stringValue.replacingOccurrences(of: "_", with: " "))
                        Text("Observed coverage: \(((heart["coverage_fraction"].numberValue ?? 0) * 100).formatted())%")
                        ForEach(Array(heart["absolute_bins"].arrayValue.enumerated()), id: \.offset) { _, bin in
                            Text("\(Int(bin["lower_bpm"].numberValue ?? 0))–\(Int(bin["upper_bpm"].numberValue ?? 0)) bpm: \(((bin["seconds"].numberValue ?? 0) / 60).formatted()) minutes")
                        }
                        ForEach(Array(heart["zone_seconds"].arrayValue.enumerated()), id: \.offset) { index, seconds in
                            Text("\(50 + index * 10)–\(60 + index * 10)%: \(((seconds.numberValue ?? 0) / 60).formatted()) minutes")
                        }
                        Text("Observed Edwards load: \(heart["edwards_load_au"].numberValue?.formatted() ?? "Not available") AU")
                        ForEach(heart["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
                    }
                    DisclosureGroup("Recent records (up to 200)") {
                        ForEach(Array(source["points"].arrayValue.suffix(200).reversed().enumerated()), id: \.offset) { _, point in
                            VStack(alignment: .leading) {
                                Text("\(point["value"].numberValue?.formatted() ?? "Not available") \(result["unit"].stringValue)")
                                if let at = point["at"].numberValue { Text(Date(timeIntervalSince1970: at).formatted()).font(.caption) }
                                Text("Original: \(point["raw_value"].numberValue?.formatted() ?? "Unknown") \(point["raw_unit"].stringValue)").font(.caption)
                            }
                        }
                    }
                }
            }
            Section("Method and coverage") {
                if !loading && failure == nil && result["sources"].arrayValue.isEmpty { Text("No recorded measurements in this window.") }
                if kind == "heart_rate" { Text("Sources stay separate. Each source includes its interval method and declared maximum.").font(.caption) }
                else { ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) } }
                if kind == "blood_glucose" { Text("Glucose summaries hold each sample until the next sample only when the gap is at most 15 minutes. Long gaps remain missing.").font(.caption) }
                NavigationLink("Export the source archive", destination: SettingsView(model: model))
            }
        }.navigationTitle("Device trends")
            .task(id: kind) { await reload() }.refreshable { await reload() }
    }
    private func reload() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let selected = kind
        loading = true
        result = .null
        failure = nil
        defer { if selected == kind { loading = false } }
        let end = Date().timeIntervalSince1970.rounded(.down)
        do {
            let gap = selected == "heart_rate" ? Double(maximumGap) : 900
            guard let gap, gap.isFinite else { throw DraftError.message("Enter a valid sample gap.") }
            var maximum: JSONValue = .null
            if selected == "heart_rate", !maximumBPM.isEmpty {
                guard let bpm = Double(maximumBPM), bpm.isFinite else { throw DraftError.message("Enter a valid maximum heart rate.") }
                maximum = .object(["bpm": .number(bpm), "source": .string(maximumSource)])
            }
            let response = try await model.request("wellness/series", body: .object([
                "record_type": .string(selected), "start_at": .number(end - (selected == "heart_rate" ? 1 : 30) * 86400), "end_at": .number(end), "maximum_gap_seconds": .number(gap), "declared_maximum": maximum
            ]))
            guard !Task.isCancelled, model.session?.token == token, selected == kind else { return }
            result = response
        } catch {
            guard !Task.isCancelled, model.session?.token == token, selected == kind else { return }
            failure = error.localizedDescription
        }
    }
}
