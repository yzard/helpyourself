import SwiftUI
import Charts

struct SpecialtyRecordsView: View {
    var model: AppModel
    @State private var kind = "ecg"
    @State private var records: [JSONValue] = []
    @State private var cursor: JSONValue = .null
    @State private var failure: String?
    @State private var loading = false
    var body: some View {
        List {
            Picker("Record type", selection: $kind) { Text("ECG").tag("ecg"); Text("Clinical and medication records").tag("clinical"); Text("Paired blood pressure").tag("blood_pressure") }
            Text("Source measurements and classifications. No new diagnostic classification is generated.").font(.caption)
            if let failure { Text(failure).foregroundStyle(.secondary) }
            if records.isEmpty && !loading { Text("No available records of this type.") }
            ForEach(Array(records.enumerated()), id: \.offset) { _, item in
                NavigationLink { SpecialtyRecordView(model: model, reference: item) } label: {
                    VStack(alignment: .leading) {
                        Text(item["record_type"].stringValue)
                        Text(Date(timeIntervalSince1970: item["at"].numberValue ?? 0).formatted()).font(.caption)
                        Text(item["platform"].stringValue + ":" + item["source_id"].stringValue).font(.caption)
                    }
                }
            }
            if cursor != .null { Button("More records") { Task { await load(reset: false) } }.disabled(loading) }
        }.navigationTitle("Specialty records").task(id: kind) { await load(reset: true) }
    }
    private func load(reset: Bool) async {
        guard let client = model.client, let token = model.session?.token else { return }
        loading = true; defer { loading = false }
        if reset { records = []; cursor = .null }
        do {
            let response = try await model.request("wellness/records/list", body: .object(["kind": .string(kind), "after": cursor]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            records += response["records"].arrayValue; cursor = response["next_cursor"]; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}

private struct SpecialtyRecordView: View {
    var model: AppModel
    let reference: JSONValue
    @State private var result: JSONValue = .null
    @State private var offset = 0
    @State private var failure: String?
    var body: some View {
        List {
            Section("Source") {
                Text(reference["platform"].stringValue + ":" + reference["source_id"].stringValue)
                Text(reference["record_id"].stringValue).font(.caption)
                Text("Source classification code: " + (result["source_classification"].numberValue?.formatted() ?? "Not available"))
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            if result["record_type"] == .string("electrocardiogram") {
                Section("Source waveform") {
                    Chart {
                        ForEach(Array(result["samples"].arrayValue.enumerated()), id: \.offset) { _, point in
                            if let time = point["time_since_start"].numberValue, let voltage = point["lead_i_volts"].numberValue {
                                PointMark(x: .value("Seconds", time), y: .value("mV", voltage * 1000)).symbolSize(2)
                            }
                        }
                    }.frame(height: 220).accessibilityLabel("Source ECG samples in millivolts. Missing samples are not connected.")
                    Text("Samples \(offset)–\(offset + result["samples"].arrayValue.count) of \(Int(result["sample_count"].numberValue ?? 0))").font(.caption)
                    if offset > 0 { Button("Previous samples") { offset = max(0, offset - 5000) } }
                    if let next = result["next_sample_offset"].numberValue { Button("Next samples") { offset = Int(next) } }
                }
            }
            if result["fhir"]["resource"] != .null {
                Section("Clinical resource") {
                    Text(result["fhir"]["resource"]["resourceType"].stringValue).font(.headline)
                    ForEach(result["fhir"]["resource"].objectValue.keys.sorted(), id: \.self) { key in
                        DisclosureGroup(key) { Text(result["fhir"]["resource"][key].selfDescription).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
                    }
                    ShareLink("Share source resource", item: result["fhir"]["resource"].selfDescription)
                }
            } else if reference["record_type"] != .string("electrocardiogram") { Text(result["fhir"]["state"].stringValue.replacingOccurrences(of: "_", with: " ")) }
            ForEach(Array(result["related"].arrayValue.enumerated()), id: \.offset) { _, relation in
                Section("Related source measurement") {
                    Text(relation["state"].stringValue)
                    ForEach(Array(relation["records"].arrayValue.enumerated()), id: \.offset) { _, record in
                        Text(record["record_type"].stringValue + " · " + (record["value"].numberValue?.formatted() ?? "Missing") + " " + record["unit"].stringValue)
                    }
                }
            }
            ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
        }.navigationTitle("Source record").task(id: offset) { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let response = try await model.request("wellness/records/get", body: .object(["platform": reference["platform"], "source_id": reference["source_id"], "record_id": reference["record_id"], "sample_offset": .number(Double(offset))]))
            guard !Task.isCancelled, model.session?.token == token else { return }; result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
