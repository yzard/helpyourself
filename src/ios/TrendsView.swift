import SwiftUI
import Charts

struct TrendsView: View {
    var model: AppModel
    @State private var selected = "ldl_cholesterol"
    @State private var comparing = false
    @State private var second = "hdl_cholesterol"
    var body: some View {
        List {
            Section("Choose metrics") {
                picker("Metric", selection: $selected)
                Toggle("Compare a second metric", isOn: $comparing)
                if comparing { picker("Second metric", selection: $second) }
            }
            TrendSeries(model: model, metric: selected).id(selected)
            if comparing && second != selected { TrendSeries(model: model, metric: second).id(second) }
        }.navigationTitle("Trends")
    }
    private func picker(_ label: String, selection: Binding<String>) -> some View {
        Picker(label, selection: selection) { ForEach(model.metrics, id: \.identifier) { Text($0["name"].stringValue).tag($0["metric_id"].stringValue) } }
    }
}

private struct TrendSeries: View {
    var model: AppModel
    let metric: String
    @State private var result: JSONValue = .null
    @State private var selectedTime: Double?
    @State private var preview: DocumentLocation?
    var body: some View {
        Group {
            Section(model.metrics.first(where: { $0["metric_id"].stringValue == metric })?["name"].stringValue ?? metric) {
                if result["points"].arrayValue.isEmpty { Text("No comparable results. Confirm a result with a mapped metric, a recognized unit and collection date.").foregroundStyle(.secondary) }
                else {
                    Chart(result["points"].arrayValue, id: \.identifier) { point in
                        LineMark(x: .value("Collection date", point["timestamp"].numberValue ?? 0), y: .value("Value", point["value"].numberValue ?? 0))
                        PointMark(x: .value("Collection date", point["timestamp"].numberValue ?? 0), y: .value("Value", point["value"].numberValue ?? 0))
                    }.chartXSelection(value: $selectedTime).chartXAxis { AxisMarks { value in AxisValueLabel { if let seconds = value.as(Double.self) { Text(Date(timeIntervalSince1970: seconds), format: .dateTime.year().month()) } } } }.frame(height: 220)
                    Text("\(result["points"].arrayValue.first?["unit"].stringValue ?? "") · tap a point or a dated result to open its original report").font(.caption)
                }
                if let count = result["incomparable_count"].numberValue, count > 0 { Text("\(Int(count)) confirmed results have an unknown date, unsupported unit or non-numeric value. They remain in Reports.").font(.caption) }
            }
            Section("Source reports") {
                Button("Refresh history") { Task { await reload() } }
                ForEach(result["points"].arrayValue, id: \.identifier) { point in
                    Button { show(point) } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(point["sampled_at"].stringValue)
                            Text("\(point["value"].stringValue) \(point["unit"].stringValue)").font(.headline)
                            Text("Reference: \(point["original"]["reference_range"].stringValue)").font(.caption)
                            Text("Original: \(point["original"]["raw_result"].stringValue) \(point["original"]["raw_unit"].stringValue) · revision \(Int(point["revision"].numberValue ?? 1))").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }.task(id: metric) { await reload() }
            .onChange(of: selectedTime) { _, time in
                if let time, let point = result["points"].arrayValue.min(by: { abs(($0["timestamp"].numberValue ?? 0) - time) < abs(($1["timestamp"].numberValue ?? 0) - time) }) { show(point); selectedTime = nil }
            }.sheet(item: $preview) { DocumentPreview(location: $0) }
    }
    private func reload() async { do { if let client = model.client { result = try await client.post("trends/get", body: .object(["metric_ids": .array([.string(metric)])])) } } catch { model.errorMessage = error.localizedDescription } }
    private func show(_ point: JSONValue) { Task { do { preview = DocumentLocation(url: try await model.source(point["report_id"].stringValue), page: Int(point["original"]["source"]["page"].numberValue ?? 1)) } catch { model.errorMessage = error.localizedDescription } } }
}
