import Foundation
import Testing
@testable import HelpYourselfCore

@Test func trainingFilePreservesBomUnicodeAndRejectsRepairedBytes() throws {
    let original = Data([0xef, 0xbb, 0xbf]) + Data("<gpx>cafe\u{301} 健康</gpx>".utf8)
    let file = try TrainingFile(filename: "track.GPX", data: original)
    #expect(Data(file.contents.utf8) == original)
    #expect(file.format == "gpx")
    #expect(throws: TrainingFileError.self) { try TrainingFile(filename: "bad.tcx", data: Data([0xff, 0x61])) }
    #expect(throws: TrainingFileError.self) { try TrainingFile(filename: "../bad.gpx", data: Data("x".utf8)) }
    #expect(throws: TrainingFileError.self) { try TrainingFile(filename: "big.tcx", data: Data(repeating: 65, count: 4 * 1024 * 1024 + 1)) }
}
