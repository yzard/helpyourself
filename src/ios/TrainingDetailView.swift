import SwiftUI
import Charts

struct TrainingDetailView: View {
    var model: AppModel
    let reference: JSONValue
    @State private var result: JSONValue = .null
    @State private var offset = 0
    @State private var failure: String?
    private let metrics = [("power_watts", "Power", "W"), ("cadence_rpm", "Cadence", "rpm"), ("heart_rate_bpm", "Heart rate", "bpm"), ("speed_mps", "Speed", "m/s")]
    var body: some View {
        List {
            Section("Source activity") {
                Text(reference["filename"].stringValue)
                Text(reference["source_id"].stringValue).font(.caption)
                if let elapsed = result["elapsed_seconds"].numberValue { Text("Elapsed: \((elapsed / 60).formatted()) minutes") }
                if let timer = result["source_total_time_seconds"].numberValue { Text("Source timer: \((timer / 60).formatted()) minutes") } else { Text("Source timer duration unknown") }
                Text("Elapsed time can include pauses. Points show source measurements, without filling gaps.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(metrics, id: \.0) { metric in
                Section(metric.1) {
                    let points = result["samples"].arrayValue
                    if points.contains(where: { $0[metric.0].numberValue != nil }) {
                        Chart {
                            ForEach(Array(points.enumerated()), id: \.offset) { _, point in
                                if let at = point["at"].numberValue, let value = point[metric.0].numberValue {
                                    PointMark(x: .value("Time", Date(timeIntervalSince1970: at)), y: .value(metric.2, value)).symbolSize(3)
                                }
                            }
                        }.frame(height: 160).accessibilityLabel("Source \(metric.1) samples in \(metric.2)")
                    } else { Text("No source values on this page") }
                }
            }
            Section("Sample pages") {
                Text("\(offset)–\(offset + result["samples"].arrayValue.count) of \(Int(result["sample_count"].numberValue ?? 0))")
                if offset > 0 { Button("Previous samples") { offset = max(0, offset - 5000) } }
                if let next = result["next_sample_offset"].numberValue { Button("Next samples") { offset = Int(next) } }
            }
            Section("Source laps and intervals") {
                if result["laps"].arrayValue.isEmpty { Text("No source lap records") }
                ForEach(Array(result["laps"].arrayValue.enumerated()), id: \.offset) { index, lap in
                    VStack(alignment: .leading) {
                        Text("Lap \(index + 1)").font(.headline)
                        if let start = lap["start_at"].numberValue { Text(Date(timeIntervalSince1970: start).formatted()).font(.caption) }
                        if let duration = lap["source_total_time_seconds"].numberValue { Text("Timer: \((duration / 60).formatted()) minutes") }
                        if let distance = lap["source_distance_m"].numberValue { Text("Distance: \(distance.formatted()) m") }
                        if let power = lap["average_power_watts"].numberValue { Text("Average power: \(power.formatted()) W") }
                    }
                }
            }
        }.navigationTitle("Training details").task(id: offset) { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let response = try await model.request("wellness/records/get", body: .object(["platform": .string("file_import"), "source_id": reference["source_id"], "record_id": reference["record_id"], "sample_offset": .number(Double(offset))]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
