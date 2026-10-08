import SwiftUI
import Charts

struct MealGlucoseView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var maximumGap = "900"
    @State private var result: JSONValue = .null
    @State private var failure: String?
    var body: some View {
        List {
            Section("Time alignment") {
                DatePicker("Date", selection: $date, displayedComponents: .date)
                TextField("Maximum sample gap (seconds)", text: $maximumGap).keyboardType(.numberPad)
                Text("900 seconds is an initial display parameter. Match the actual source sampling protocol.").font(.caption)
                Button("Apply sampling gap") { Task { await load() } }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(Array(result["series"]["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                Section(source["source"].stringValue) {
                    Chart {
                        ForEach(Array(source["points"].arrayValue.enumerated()), id: \.offset) { _, point in
                            if let at = point["at"].numberValue, let value = point["value"].numberValue {
                                PointMark(x: .value("Time", Date(timeIntervalSince1970: at)), y: .value("mg/dL", value)).symbolSize(8)
                            }
                        }
                        ForEach(Array(result["events"].arrayValue.enumerated()), id: \.offset) { _, event in
                            if let at = event["at"].numberValue {
                                RuleMark(x: .value("Event", Date(timeIntervalSince1970: at))).foregroundStyle(event["entry"]["kind"] == .string("nutrition") ? .orange : .purple).lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                            }
                        }
                    }.frame(height: 230).accessibilityLabel("Glucose source measurements with meal and training times. No causal relationship is estimated.")
                    Text("Orange: meal · Purple: training · Glucose: mg/dL").font(.caption)
                    Text(source["state"].stringValue.replacingOccurrences(of: "_", with: " "))
                    if let coverage = source["glucose_summary"]["coverage_fraction"].numberValue { Text("Observed interval coverage: \((coverage * 100).formatted())%") }
                }
            }
            Section("Recorded events") {
                ForEach(Array(result["events"].arrayValue.enumerated()), id: \.offset) { _, event in
                    VStack(alignment: .leading) {
                        Text(Date(timeIntervalSince1970: event["at"].numberValue ?? 0).formatted(date: .omitted, time: .shortened))
                        Text(event["entry"]["kind"] == .string("nutrition") ? event["entry"]["content"]["food"].stringValue : "Training")
                        if let carbohydrate = event["entry"]["content"]["carbohydrate_g"].numberValue { Text("Recorded carbohydrate: \(carbohydrate.formatted()) g").font(.caption) }
                    }
                }
                NavigationLink("Correct recorded events", destination: DailyLogsView(model: model))
            }
            Section("Interpretation") { ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) } }
        }.navigationTitle("Meals and glucose").task(id: date) { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        guard let gap = Double(maximumGap), gap.rounded() == gap, (1...1800).contains(gap) else { failure = "Enter a gap from 1 to 1800 seconds."; return }
        let start = Calendar.current.startOfDay(for: date)
        guard let end = Calendar.current.date(byAdding: .day, value: 1, to: start) else { return }
        do {
            let response = try await model.request("wellness/meal-glucose", body: .object(["start_at": .number(start.timeIntervalSince1970), "end_at": .number(end.timeIntervalSince1970), "maximum_gap_seconds": .number(gap)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
