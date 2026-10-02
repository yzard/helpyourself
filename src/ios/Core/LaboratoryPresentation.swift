/// Formats server-derived laboratory evidence without implementing conversion rules on the phone.
public nonisolated enum LaboratoryPresentation {
    public static func originalReference(_ payload: JSONValue, reference: JSONValue) -> String {
        let text = payload["reference_range"].stringValue
        guard !text.isEmpty else { return "Original reference: not provided" }
        let unit = reference["original_unit"].stringValue
        if unit.isEmpty { return "Original reference: \(text) · unit unknown" }
        let origin = reference["unit_origin"].stringValue == "reference_explicit" ? "explicit unit" : "result-column unit"
        return "Original reference: \(text) · \(unit) (\(origin))"
    }

    public static func standardizedReference(_ reference: JSONValue) -> String {
        let display = reference["display"].stringValue
        guard !display.isEmpty else {
            let reason = reference["reason"].stringValue
            return reason.isEmpty ? "Standardized reference unavailable" : "Reference retained as printed: \(reason)"
        }
        return "Standardized reference: \(display)"
    }

    public static func resultExplanation(_ interpretation: JSONValue) -> String {
        let display = interpretation["result"]["display"].stringValue
        if !display.isEmpty { return "Standardized result: \(display)" }
        return interpretation["result"]["reason"].stringValue
    }
}
