import SwiftUI

struct NutritionDayView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var result: JSONValue = .null
    @State private var failure: String?
    var body: some View {
        List {
            Section {
                DatePicker("Date", selection: $date, displayedComponents: .date)
                NavigationLink("Meal planner and shopping list", destination: MealPlannerView(model: model))
                NavigationLink("Meals and glucose", destination: MealGlucoseView(model: model))
                NavigationLink("Diet quality assessment", destination: DietQualityView(model: model))
                NavigationLink("Foods, recipes and goals", destination: FoodLibraryView(model: model))
                Text("Recorded intake only. Missing entries remain unknown.").font(.caption)
                Text(result["goal_state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(Array(result["nutrients"].arrayValue.enumerated()), id: \.offset) { _, item in
                Section(item["nutrient"].stringValue.replacingOccurrences(of: "_", with: " ")) {
                    Text((item["observed_sum"].numberValue?.formatted() ?? "Unknown") + " " + item["unit"].stringValue)
                    Text("Known records: \(Int(item["known_records"].numberValue ?? 0)) · Missing values: \(Int(item["missing_records"].numberValue ?? 0))").font(.caption)
                    if let target = item["target"].numberValue { Text("Your target: \(target.formatted()) \(item["unit"].stringValue)") }
                    if let fraction = item["recorded_fraction_of_target"].numberValue { Text("Recorded intake: \((100 * fraction).formatted())% of target").font(.caption) }
                }
            }
            Section("Recorded foods") {
                ForEach(Array(result["records"].arrayValue.enumerated()), id: \.offset) { _, item in
                    Text(item["entry"]["content"]["food"].stringValue + " · " + item["entry"]["content"]["meal"].stringValue)
                }
            }
            ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
        }.navigationTitle("Nutrition").task(id: date) { await load() }.refreshable { await load() }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let formatter = DateFormatter(); formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.dateFormat = "yyyy-MM-dd"; formatter.timeZone = .current
        do {
            let response = try await model.request("wellness/nutrition/day", body: .object(["date": .string(formatter.string(from: date)), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }; result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
