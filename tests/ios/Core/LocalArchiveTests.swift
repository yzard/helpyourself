import Foundation
import Testing
@testable import HelpYourselfCore

@Test func checkpointSurvivesRelaunchAndArchiveRejectsTraversal() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let archive = try LocalArchive(directory: directory)
    defer { try? archive.clear() }
    let checkpoint = SyncCheckpoint(requests: [.object(["batch_id": .string("stable")])], nextAnchor: Data([1,2,3]))
    try archive.save(checkpoint, name: "pending.json")
    let reopened = try LocalArchive(directory: directory)
    let loaded = try #require(try reopened.load(SyncCheckpoint.self, name: "pending.json"))
    #expect(loaded.requests == checkpoint.requests)
    #expect(loaded.nextAnchor == checkpoint.nextAnchor)
    #expect(throws: (any Error).self) { try archive.location("../private") }
    #expect(throws: (any Error).self) { try archive.location("..") }
    try archive.remove(name: "pending.json")
    #expect(try archive.load(SyncCheckpoint.self, name: "pending.json") == nil)
}
