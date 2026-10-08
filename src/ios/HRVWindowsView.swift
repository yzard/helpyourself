import SwiftUI
import Charts

struct HRVWindowsView: View {
    var model: AppModel
    @State private var result: JSONValue = .null
    @State private var failure: String?
    var body: some View {
        List {
            Section {
                Text("Five-minute normal-to-normal intervals. SDNN stays a separate measurement.")
                Text("Unknown quality and gaps exclude a window. Device or protocol changes start a separate baseline.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
                if result["sources"].arrayValue.isEmpty { Text("No archived NN or heartbeat windows in the last 90 days.") }
            }
            ForEach(Array(result["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                Section(source["source"].stringValue) {
                    ForEach(["device_model", "firmware", "protocol_id", "context", "posture"], id: \.self) { key in
                        LabeledContent(key.replacingOccurrences(of: "_", with: " ").capitalized, value: source["protocol"][key].stringValue.isEmpty ? "Unknown" : source["protocol"][key].stringValue).font(.caption)
                    }
                    Chart {
                        ForEach(Array(source["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                            if let value = day["ln_rmssd"].numberValue {
                                PointMark(x: .value("Date", day["date"].stringValue), y: .value("lnRMSSD", value))
                            }
                        }
                    }.frame(height: 200).accessibilityLabel("Daily lnRMSSD, natural logarithm of RMSSD in milliseconds")
                    ForEach(Array(source["days"].arrayValue.enumerated()), id: \.offset) { _, day in
                        VStack(alignment: .leading) {
                            Text(day["date"].stringValue + " · lnRMSSD " + (day["ln_rmssd"].numberValue?.formatted() ?? "Unknown"))
                            if let week = day["seven_day_median"].numberValue { Text("Seven-day median: \(week.formatted())").font(.caption) }
                            Text("Baseline days: \(Int(day["baseline_days"].numberValue ?? 0))/28").font(.caption)
                            if let median = day["baseline"]["median"].numberValue { Text("Prior median: \(median.formatted())").font(.caption) }
                        }
                    }
                    DisclosureGroup("Source windows and exclusion reasons") {
                        ForEach(Array(source["windows"].arrayValue.enumerated()), id: \.offset) { _, window in
                            VStack(alignment: .leading) {
                                Text(window["date"].stringValue + " · " + window["state"].stringValue)
                                Text(window["reason"].stringValue).font(.caption)
                                Text("Record: " + window["record_id"].stringValue).font(.caption)
                            }
                        }
                    }
                }
            }
        }.navigationTitle("HRV windows").task { await load() }.refreshable { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let now = floor(Date().timeIntervalSince1970)
            let response = try await model.request("wellness/hrv", body: .object(["start_at": .number(now - 90 * 86400), "end_at": .number(now), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
