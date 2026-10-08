import SwiftUI
import Charts

struct ComparisonView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var first = "sleep"
    @State private var second = "resting_heart_rate"
    @State private var firstSource = ""
    @State private var secondSource = ""
    @State private var selectedDate: String?
    @State private var result: JSONValue = .null
    @State private var failure: String?
    private let metrics = ["sleep", "workout", "steps", "resting_heart_rate", "heart_rate", "hrv_sdnn"]
    var body: some View {
        List {
            Section("Compare daily observations") {
                DatePicker("Ending date", selection: $date, displayedComponents: .date)
                Text("Thirty calendar days. Each chart retains its own units and source. Aligned changes do not establish a causal relationship.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            comparison(metric: $first, source: $firstSource, label: "First measurement")
            comparison(metric: $second, source: $secondSource, label: "Second measurement")
            Section("Inspect one day") {
                Picker("Date", selection: $selectedDate) {
                    Text("Choose a date").tag(String?.none)
                    ForEach(dates, id: \.self) { Text($0).tag(Optional($0)) }
                }
                if let selectedDate {
                    Text(value(metric: first, source: firstSource, date: selectedDate))
                    Text(value(metric: second, source: secondSource, date: selectedDate))
                }
                Text("Days without observations stay missing. Missing points do not become zero or connect across gaps.").font(.caption)
            }
        }.navigationTitle("Compare measurements").task(id: date) { await load() }
    }
    private var dates: [String] {
        let formatter = dateFormatter
        let end = Calendar.current.startOfDay(for: date)
        return (0..<30).reversed().compactMap { Calendar.current.date(byAdding: .day, value: -$0, to: end) }.map(formatter.string)
    }
    private var dateFormatter: DateFormatter {
        let value = DateFormatter(); value.locale = Locale(identifier: "en_US_POSIX"); value.dateFormat = "yyyy-MM-dd"; value.timeZone = .current; return value
    }
    private func sources(_ metric: String) -> [JSONValue] { result["metrics"].arrayValue.first { $0["record_type"] == .string(metric) }?["sources"].arrayValue ?? [] }
    private func selected(_ metric: String, _ source: String) -> JSONValue { sources(metric).first { $0["source"] == .string(source) } ?? .null }
    private func value(metric: String, source: String, date: String) -> String {
        let record = selected(metric, source)["days"].arrayValue.first { $0["date"] == .string(date) } ?? .null
        return metric.replacingOccurrences(of: "_", with: " ") + ": " + (record["value"].numberValue?.formatted() ?? "Missing") + " " + record["unit"].stringValue
    }
    private func comparison(metric: Binding<String>, source: Binding<String>, label: String) -> some View {
        Section(label) {
            Picker("Measurement", selection: metric) { ForEach(metrics, id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) } }
            Picker("Source", selection: source) {
                Text("Choose a source").tag("")
                ForEach(Array(sources(metric.wrappedValue).enumerated()), id: \.offset) { _, item in Text(item["source"].stringValue).tag(item["source"].stringValue) }
            }
            let record = selected(metric.wrappedValue, source.wrappedValue)
            if record == .null { Text("Choose a source with observed values") }
            else {
                Chart {
                    ForEach(Array(record["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                        if let value = day["value"].numberValue {
                            PointMark(x: .value("Date", day["date"].stringValue), y: .value(day["unit"].stringValue, value))
                        }
                    }
                    if let selectedDate { RuleMark(x: .value("Selected day", selectedDate)).foregroundStyle(.secondary) }
                }.chartXScale(domain: dates).chartXAxis(.hidden).frame(height: 190)
                Text(record["days"].arrayValue.first?["unit"].stringValue ?? "").font(.caption)
            }
        }.onChange(of: metric.wrappedValue) { _, _ in source.wrappedValue = "" }
    }
    private func load() async {
        guard let token = model.session?.token else { return }
        let end = Calendar.current.startOfDay(for: date)
        guard let start = Calendar.current.date(byAdding: .day, value: -29, to: end) else { return }
        do {
            let response = try await model.request("wellness/report", body: .object(["start_date": .string(dateFormatter.string(from: start)), "end_date": .string(dateFormatter.string(from: end)), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
