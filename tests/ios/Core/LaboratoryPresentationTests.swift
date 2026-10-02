import Testing
@testable import HelpYourselfCore

@Test func referenceKeepsOriginalAndStandardUnitsSeparate() {
    let payload: JSONValue = .object(["reference_range": .string("<2.586")])
    let reference: JSONValue = .object(["original_unit": .string("mmol/L"), "unit_origin": .string("observation"), "display": .string("< 100 mg/dL")])
    #expect(LaboratoryPresentation.originalReference(payload, reference: reference).contains("mmol/L (result-column unit)"))
    #expect(LaboratoryPresentation.standardizedReference(reference) == "Standardized reference: < 100 mg/dL")
    #expect(LaboratoryPresentation.originalReference(payload, reference: reference).contains("<2.586"))
}

@Test func unsupportedReferenceDoesNotInventAStandardRange() {
    let reference: JSONValue = .object(["reason": .string("Contextual reference requires review.")])
    #expect(LaboratoryPresentation.standardizedReference(reference) == "Reference retained as printed: Contextual reference requires review.")
    #expect(LaboratoryPresentation.originalReference(.object(["reference_range": .string("Age dependent")]), reference: reference).contains("unit unknown"))
}
