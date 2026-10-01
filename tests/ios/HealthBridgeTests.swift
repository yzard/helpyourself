import Testing
import Foundation
import HealthKit
@testable import Helpyourself

@MainActor @Test func healthDefinitionsUseUniqueTypesAndExplicitCoverage() {
    let bridge = HealthBridge()
    let kinds = bridge.definitions.map(\.kind)
    #expect(Set(kinds).count == kinds.count)
    #expect(kinds.contains("sleep"))
    #expect(kinds.contains("workout"))
    #expect(kinds.contains("hrv_sdnn"))
    #expect(!bridge.isSyncing)
    #expect(kinds.contains("electrocardiogram"))
    #expect(kinds.contains("workout_route"))
    #expect(Set(bridge.definitions.map { $0.type.identifier }).count == bridge.definitions.count)
}

@Test func rawHealthSampleCanBeRestoredWithoutLosingMetadata() throws {
    let type = try #require(HKObjectType.quantityType(forIdentifier: .bodyMass))
    let sample = HKQuantitySample(type: type, quantity: HKQuantity(unit: .pound(), doubleValue: 150), start: Date(timeIntervalSince1970: 100.25), end: Date(timeIntervalSince1970: 101.75), metadata: ["raw_flag": true, "nested": ["value": "original"]])
    let record = try HealthBridge.encode(sample, kind: "body_mass", unit: "kg")
    let bytes = try #require(Data(base64Encoded: record["payload"]["raw_archive"]["data"].stringValue))
    let restored = try #require(try NSKeyedUnarchiver.unarchivedObject(ofClass: HKQuantitySample.self, from: bytes))
    #expect(restored.uuid == sample.uuid)
    #expect(restored.startDate == sample.startDate)
    #expect(restored.quantity.doubleValue(for: .pound()) == 150)
    #expect(restored.metadata?["raw_flag"] as? Bool == true)
}
