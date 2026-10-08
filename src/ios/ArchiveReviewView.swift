import SwiftUI

struct ArchiveReviewView: View {
    var model: AppModel
    @State private var result: JSONValue = .null
    @State private var failure: String?
    var body: some View {
        List {
            Section {
                Text("Last 30 days · \(TimeZone.current.identifier)")
                NavigationLink("Record or edit a log", destination: DailyLogsView(model: model))
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            Section("Training load · completed calendar days") {
                ForEach(Array(result["training"]["windows"].arrayValue.enumerated()), id: \.offset) { _, window in
                    VStack(alignment: .leading) {
                        Text("\(Int(window["days"].numberValue ?? 0)) days").font(.headline)
                        Text("Recorded load: \(window["observed_sum_au"].numberValue?.formatted() ?? "Unknown") AU")
                        Text("Complete days: \(Int(window["complete_days"].numberValue ?? 0)) / \(Int(window["days"].numberValue ?? 0))")
                        Text("Daily mean: \(window["daily_mean_au"].numberValue?.formatted() ?? "Unknown") AU")
                    }
                }
                ForEach(result["training"]["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
            }
            ForEach(Array(result["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                Section(day["date"].stringValue + " · " + day["kind"].stringValue) {
                    Text("\(Int(day["record_count"].numberValue ?? 0)) recorded entries").font(.caption)
                    ForEach(Array(day["totals"].arrayValue.enumerated()), id: \.offset) { _, total in
                        VStack(alignment: .leading) {
                            Text(total["metric"].stringValue.replacingOccurrences(of: "_", with: " "))
                            Text("\(total["observed_sum"].numberValue?.formatted() ?? "Not available") \(total["unit"].stringValue)").font(.headline)
                            Text("Missing from \(Int(total["missing_count"].numberValue ?? 0)) logs").font(.caption)
                        }
                    }
                }
            }
            Section("Recorded strength bests in this window") {
                ForEach(Array(result["strength_bests"].arrayValue.enumerated()), id: \.offset) { _, best in
                    Text("\(best["exercise"].stringValue) · \(Int(best["repetitions"].numberValue ?? 0)) repetitions · \(best["external_weight_kg"].numberValue?.formatted() ?? "") kg")
                }
            }
            Section("Body and cycle observations") {
                ForEach(Array(result["measurements"].arrayValue.filter { ["body", "blood_pressure", "cycle"].contains($0["entry"]["kind"].stringValue) }.enumerated()), id: \.offset) { _, record in
                    VStack(alignment: .leading, spacing: 6) {
                        if let at = record["at"].numberValue { Text(Date(timeIntervalSince1970: at).formatted()).font(.caption) }
                        let content = record["entry"]["content"]
                        if record["entry"]["kind"] == .string("body") {
                            Text("Weight: \(content["weight_kg"].numberValue?.formatted() ?? "Unknown") kg")
                        } else if record["entry"]["kind"] == .string("blood_pressure") {
                            Text("\(content["systolic_mmhg"].numberValue?.formatted() ?? "Unknown") / \(content["diastolic_mmhg"].numberValue?.formatted() ?? "Unknown") mmHg")
                            Text("\(content["posture"].stringValue) · \(content["arm"].stringValue)").font(.caption)
                        } else {
                            Text("Flow: \(content["flow"].stringValue) · \(content["context"].stringValue)")
                            Text(content["symptoms"].arrayValue.map(\.stringValue).joined(separator: ", ")).font(.caption)
                        }
                    }
                }
            }
            Section("Method") {
                ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
                if let data = try? JSONEncoder().encode(result), let text = String(data: data, encoding: .utf8), result != .null {
                    ShareLink("Share review and input references", item: text)
                }
            }
        }.navigationTitle("Log review").task { await reload() }.refreshable { await reload() }
    }
    private func reload() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let end = Date().timeIntervalSince1970.rounded(.down)
        do {
            let response = try await model.request("wellness/review", body: .object(["start_at": .number(end - 30 * 86400), "end_at": .number(end), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch {
            guard !Task.isCancelled, model.session?.token == token else { return }
            failure = error.localizedDescription
        }
    }
}

struct ArchiveTimelineView: View {
    var model: AppModel
    @State private var events: [JSONValue] = []
    @State private var cursor: JSONValue = .null
    @State private var end = Date().timeIntervalSince1970.rounded(.down)
    @State private var failure: String?
    @State private var loading = false
    var body: some View {
        List {
            Section {
                Text("Last 7 days. Report dates are upload dates. Sleep stages remain individual original intervals.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
                Button("Refresh timeline") { Task { await load(reset: true) } }.disabled(loading)
            }
            ForEach(Array(events.enumerated()), id: \.offset) { _, event in
                VStack(alignment: .leading, spacing: 6) {
                    Text(event["record_type"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.headline)
                    if let at = event["at"].numberValue { Text(Date(timeIntervalSince1970: at).formatted()) }
                    Text("\(event["source"].stringValue) · revision \(Int(event["version"].numberValue ?? 0))").font(.caption)
                    if event["source"] == .string("manual:helpyourself") { NavigationLink("Open logs", destination: DailyLogsView(model: model)) }
                    else if event["record_type"] == .string("sleep") { NavigationLink("Open sleep sessions", destination: SleepSessionsView(model: model)) }
                    else if event["record_type"] != .string("report") { NavigationLink("Open source records", destination: HealthView(model: model)) }
                }
            }
            if cursor != .null { Button("Load more events") { Task { await load(reset: false) } }.disabled(loading) }
            if loading { ProgressView() }
        }.navigationTitle("Timeline").task { await load(reset: true) }
    }
    private func load(reset: Bool) async {
        guard !loading, let client = model.client, let token = model.session?.token else { return }
        loading = true
        defer { loading = false }
        if reset { end = Date().timeIntervalSince1970.rounded(.down); cursor = .null; events = [] }
        do {
            let response = try await model.request("wellness/timeline", body: .object(["start_at": .number(end - 7 * 86400), "end_at": .number(end), "cursor": cursor]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            events.append(contentsOf: response["events"].arrayValue); cursor = response["next_cursor"]; failure = nil
        } catch {
            guard !Task.isCancelled, model.session?.token == token else { return }
            failure = error.localizedDescription
        }
    }
}
