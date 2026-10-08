import SwiftUI
import Charts

struct HealthReportView: View {
    var model: AppModel
    @State private var start = Calendar.current.date(byAdding: .month, value: -3, to: Date()) ?? Date()
    @State private var end = Date()
    @State private var result: JSONValue = .null
    @State private var loading = false
    @State private var failure: String?
    var body: some View {
        List {
            Section("Report period") {
                DatePicker("Start", selection: $start, displayedComponents: .date)
                DatePicker("End", selection: $end, displayedComponents: .date)
                Text("Up to 366 calendar days. Sources remain separate.").font(.caption)
                Button("Generate report") { Task { await load() } }.disabled(loading)
                if loading { ProgressView() }
                if let failure { Text(failure).foregroundStyle(.secondary) }
                if result != .null { ShareLink("Share readable report", item: readableReport) }
            }
            ForEach(Array(result["metrics"].arrayValue.enumerated()), id: \.offset) { _, metric in
                Section(metric["record_type"].stringValue.replacingOccurrences(of: "_", with: " ")) {
                    if metric["sources"].arrayValue.isEmpty { Text(metric["state"].stringValue == "query_limit_exceeded" ? "Too many records. Select a shorter period." : "No observations") }
                    ForEach(Array(metric["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                        Text(source["source"].stringValue).font(.headline)
                        Text("Daily median: \(source["daily_median"].numberValue?.formatted() ?? "Unknown") \(source["days"].arrayValue.first?["unit"].stringValue ?? "")")
                        Text("\(Int(source["observed_days"].numberValue ?? 0)) observed days · \(Int(source["missing_days"].numberValue ?? 0)) missing days").font(.caption)
                        Chart {
                            ForEach(Array(source["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                                if let value = day["value"].numberValue {
                                    PointMark(x: .value("Date", day["date"].stringValue), y: .value(day["unit"].stringValue, value))
                                }
                            }
                        }.frame(height: 150).chartXAxis(.hidden)
                        DisclosureGroup("Daily values and units") {
                            ForEach(Array(source["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                                Text("\(day["date"].stringValue): \(day["value"].numberValue?.formatted() ?? "Unknown") \(day["unit"].stringValue)")
                            }
                        }
                    }
                }
            }
            if result != .null {
                Section("Manual records") {
                    Text("\(result["manual_records"].arrayValue.count) records in this period")
                    NavigationLink("Review recorded measurements", destination: ArchiveReviewView(model: model))
                }
                Section("Interpretation") { ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) } }
            }
        }.navigationTitle("Health report")
    }
    private var readableReport: String {
        var lines = ["Helpyourself health report", "\(result["start_date"].stringValue) to \(result["end_date"].stringValue) (\(result["timezone"].stringValue))", "Archive revision: \(result["data_revision"].numberValue?.formatted() ?? "Unknown")"]
        for metric in result["metrics"].arrayValue {
            lines.append("\n" + metric["record_type"].stringValue)
            for source in metric["sources"].arrayValue {
                lines.append("Source: " + source["source"].stringValue)
                for day in source["days"].arrayValue { lines.append("\(day["date"].stringValue): \(day["value"].numberValue?.formatted() ?? "Unknown") \(day["unit"].stringValue)") }
            }
        }
        lines += result["notes"].arrayValue.map(\.stringValue)
        return lines.joined(separator: "\n")
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        loading = true; defer { loading = false }
        let formatter = DateFormatter(); formatter.dateFormat = "yyyy-MM-dd"; formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = .current
        do {
            let response = try await model.request("wellness/report", body: .object(["start_date": .string(formatter.string(from: start)), "end_date": .string(formatter.string(from: end)), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
