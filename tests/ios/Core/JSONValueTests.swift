import Foundation
import Testing
@testable import HelpYourselfCore

@Test func nestedJSONRoundTripAndStableObservationIdentity() throws {
    let source = Data(#"{"report_id":"r","observation_id":"o","value":"1.23","active":true,"empty":null,"rows":[2]}"#.utf8)
    var value = try JSONDecoder().decode(JSONValue.self, from: source)
    #expect(value.identifier == "o")
    #expect(value["value"].numberValue == 1.23)
    #expect(value["active"].boolValue)
    #expect(value["missing"] == .null)
    value["source"]["page"] = .number(3)
    #expect(try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(value)) == value)
}
