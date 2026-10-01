import Foundation
import Testing
@testable import HelpYourselfCore

@Test func serverAddressEnforcesTransportAndRejectsEmbeddedSecrets() throws {
    #expect(try APIClient.serverURL(" https://health.example.com/base ").path == "/base")
    #expect(try APIClient.serverURL("http://localhost:8080").host == "localhost")
    for input in ["http://example.com", "https://secret@example.com", "https://example.com?key=secret", "https://example.com#secret", "file:///tmp"] {
        #expect(throws: (any Error).self) { try APIClient.serverURL(input) }
    }
}
