import SwiftUI

struct NumberMapEditor: View {
    @Binding var value: String
    let definitions: [(String, String)]
    static let nutrients = [
        ("energy_kcal", "Energy (kcal)"), ("protein_g", "Protein (g)"), ("carbohydrate_g", "Carbohydrate (g)"),
        ("fat_g", "Fat (g)"), ("fiber_g", "Fiber (g)"), ("water_ml", "Water (mL)"),
        ("saturated_fat_g", "Saturated fat (g)"), ("sugar_g", "Total sugar (g)"), ("sodium_mg", "Sodium (mg)"),
        ("potassium_mg", "Potassium (mg)"), ("calcium_mg", "Calcium (mg)"), ("iron_mg", "Iron (mg)"),
        ("magnesium_mg", "Magnesium (mg)"), ("vitamin_c_mg", "Vitamin C (mg)"), ("vitamin_d_ug", "Vitamin D (µg)"),
        ("vitamin_b12_ug", "Vitamin B12 (µg)"), ("folate_ug", "Folate (µg)")
    ]
    static let dietFields = [
        ("energy_kcal", "Energy (kcal)"), ("total_fruit_cup_eq", "Total fruit (cup equivalents)"),
        ("whole_fruit_cup_eq", "Whole fruit (cup equivalents)"), ("vegetables_cup_eq", "Vegetables (cup equivalents)"),
        ("greens_beans_cup_eq", "Greens and beans (cup equivalents)"), ("whole_grains_oz_eq", "Whole grains (oz equivalents)"),
        ("dairy_cup_eq", "Dairy (cup equivalents)"), ("protein_foods_oz_eq", "Protein foods (oz equivalents)"),
        ("seafood_plant_oz_eq", "Seafood and plant protein (oz equivalents)"), ("unsaturated_fat_g", "Mono- and polyunsaturated fat (g)"),
        ("saturated_fat_g", "Saturated fat (g)"), ("refined_grains_oz_eq", "Refined grains (oz equivalents)"),
        ("sodium_mg", "Sodium (mg)"), ("added_sugars_tsp_eq", "Added sugar (teaspoon equivalents)")
    ]
    var body: some View {
        DisclosureGroup("Amounts and units") {
            Text("Leave unknown amounts blank. Enter zero only when the source confirms zero.").font(.caption)
            ForEach(definitions, id: \.0) { key, label in
                TextField(label, text: Binding(get: { values[key] ?? "" }, set: { amount in
                    var updated = values
                    if amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { updated.removeValue(forKey: key) } else { updated[key] = amount }
                    value = updated.keys.sorted().map { "\($0) = \(updated[$0] ?? "")" }.joined(separator: "\n")
                })).keyboardType(.decimalPad)
            }
        }
    }
    private var values: [String: String] {
        var fields: [String: String] = [:]
        for line in value.split(separator: "\n") {
            let pair = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            if pair.count == 2 { fields[pair[0].trimmingCharacters(in: .whitespaces)] = pair[1].trimmingCharacters(in: .whitespaces) }
        }
        return fields
    }
}
