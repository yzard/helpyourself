import XCTest

nonisolated final class SmokeTests: XCTestCase {
    @MainActor private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        for _ in 0..<5 {
            if element.exists && element.isHittable { return }
            app.swipeUp()
        }
    }
    @MainActor private func tapButton(_ app: XCUIApplication, _ name: String) {
        let frame = app.buttons[name].frame
        app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: frame.midX, dy: frame.midY)).tap()
    }
    @MainActor private func tapTab(_ app: XCUIApplication, _ name: String) {
        let tab = name == "Health" ? "Data" : name == "Settings" ? "Overview" : name
        let frame = app.tabBars.buttons[tab].frame
        app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: frame.midX, dy: frame.midY)).tap()
        if name == "Health", app.buttons["Apple Health and sync coverage"].exists { app.buttons["Apple Health and sync coverage"].tap() }
        if name == "Settings", !app.navigationBars["Settings"].exists { app.buttons["Settings"].firstMatch.tap() }
    }
    @MainActor func testLoginTabsAndHealthAuthorization() async throws {
        addUIInterruptionMonitor(withDescription: "Synthetic password prompt") { alert in
            if alert.buttons["Not Now"].exists { alert.buttons["Not Now"].tap(); return true }
            return false
        }
        let app = XCUIApplication()
        app.launch()
        if app.buttons["Not Now"].exists { app.buttons["Not Now"].tap() }
        if app.buttons["Settings"].exists {
            app.buttons["Settings"].tap()
            app.buttons["Sign out and clear this device"].tap()
        }
        XCTAssertTrue(app.textFields["https://health.example.com"].waitForExistence(timeout: 10))
        app.textFields["https://health.example.com"].tap()
        let oldServer = app.textFields["https://health.example.com"].value as? String ?? ""
        if oldServer != "https://health.example.com" {
            app.textFields["https://health.example.com"].typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: oldServer.count))
        }
        app.textFields["https://health.example.com"].typeText("http://localhost:18765")
        app.textFields["Username"].tap()
        app.textFields["Username"].typeText("simulator")
        app.secureTextFields["Password"].tap()
        app.secureTextFields["Password"].typeText("synthetic-test-password")
        app.buttons["Sign in"].tap()
        XCTAssertTrue(app.tabBars.buttons["Archive"].waitForExistence(timeout: 20))
        tapTab(app, "Archive")
        if app.buttons["Not Now"].waitForExistence(timeout: 3) { app.buttons["Not Now"].tap() }
        XCTAssertTrue(app.staticTexts["Add your first report"].exists)
        // Relaunch after the system password sheet to verify Keychain restoration.
        app.terminate()
        app.launch()
        XCTAssertTrue(app.tabBars.buttons["Archive"].waitForExistence(timeout: 10))
        tapTab(app, "Archive")
        app.buttons["Add"].tap()
        XCTAssertTrue(app.buttons["Import PDF or image"].waitForExistence(timeout: 5))
        app.buttons["Import PDF or image"].tap()
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 10))
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.buttons["Cancel"].waitForNonExistence(timeout: 5))
        app.buttons["Add"].tap()
        XCTAssertTrue(app.buttons["Choose photo"].waitForExistence(timeout: 5))
        app.buttons["Choose photo"].tap()
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 10))
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.buttons["Cancel"].waitForNonExistence(timeout: 5))
        let system = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        if system.buttons["Not Now"].exists { system.buttons["Not Now"].tap() }
        tapTab(app, "Trends")
        XCTAssertTrue(app.switches["Compare a second metric"].waitForExistence(timeout: 5))
        app.switches["Compare a second metric"].coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", "Second metric")).firstMatch.waitForExistence(timeout: 5))
        tapTab(app, "Insights")
        XCTAssertTrue(app.staticTexts["Personal research preview"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Review lipid history and recent health data"].isEnabled)
        tapTab(app, "Health")
        XCTAssertTrue(app.buttons["Connect Apple Health"].waitForExistence(timeout: 5))
        app.buttons["Connect Apple Health"].tap()
        // Exercise the actual simulator Health permission UI, including denial.
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        if springboard.buttons["Cancel"].waitForExistence(timeout: 8) { springboard.buttons["Cancel"].tap() }
        if app.buttons["Cancel"].exists { app.buttons["Cancel"].tap() }
        if app.alerts.firstMatch.waitForExistence(timeout: 5) { app.alerts.buttons["OK"].tap() }
        var seed = URLRequest(url: URL(string: "http://127.0.0.1:18765/api/v1/test/seed-report")!)
        seed.httpMethod = "POST"
        _ = try await URLSession.shared.data(for: seed)
        app.terminate()
        app.launch()
        tapTab(app, "Archive")
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "fixture.png")).firstMatch.waitForExistence(timeout: 10))
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "fixture.png")).firstMatch.tap()
        XCTAssertTrue(app.buttons["Collection context"].waitForExistence(timeout: 10))
        app.buttons["Collection context"].tap()
        XCTAssertTrue(app.navigationBars["Collection context"].waitForExistence(timeout: 5))
        tapButton(app, "Save")
        XCTAssertTrue(app.navigationBars["Collection context"].waitForNonExistence(timeout: 10))
        app.buttons["View original report"].tap()
        XCTAssertTrue(app.navigationBars["Original · page 1"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Preview unavailable"].exists)
        tapButton(app, "Done")
        XCTAssertTrue(app.navigationBars["Original · page 1"].waitForNonExistence(timeout: 5))
        app.buttons["Save to Apple Health"].tap()
        XCTAssertTrue(app.buttons["Save reviewed glucose"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Save reviewed glucose"].isEnabled)
        app.switches["I verified the collection date and time"].coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        app.buttons["Save reviewed glucose"].tap()
        let glucoseAccess = app.switches["UIA.Health.BloodGlucose.SwitchCell.Switch"]
        if glucoseAccess.waitForExistence(timeout: 5) {
            if glucoseAccess.value as? String == "0" { glucoseAccess.tap() }
            XCTAssertTrue(app.buttons["Allow"].isEnabled)
            app.buttons["Allow"].tap()
        }
        XCTAssertTrue(app.navigationBars["Save to Apple Health"].waitForNonExistence(timeout: 15))
        XCTAssertTrue(app.buttons["Collection context"].waitForExistence(timeout: 15))
        tapTab(app, "Health")
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@ OR label CONTAINS %@", "saved to Apple Health", "Sync complete", "Some types could not be read")).firstMatch.waitForExistence(timeout: 15))
        tapTab(app, "Settings")
        XCTAssertTrue(app.buttons["Create ZIP export"].waitForExistence(timeout: 10))
        app.buttons["Create ZIP export"].tap()
        XCTAssertTrue(app.buttons["Download and share"].waitForExistence(timeout: 10))
        app.buttons["Download and share"].tap()
        XCTAssertTrue(app.staticTexts["Your complete archive is ready"].waitForExistence(timeout: 10))
        tapButton(app, "Done")
        app.buttons["Sign out and clear this device"].tap()
        XCTAssertTrue(app.buttons["Sign in"].waitForExistence(timeout: 10))
    }
    @MainActor func testReportEditingEvidenceRelationshipsAndAnalysis() async throws {
        func control(_ path: String) async throws {
            var request = URLRequest(url: URL(string: "http://127.0.0.1:18765/api/v1/\(path)")!)
            request.httpMethod = "POST"
            _ = try await URLSession.shared.data(for: request)
        }
        try await control("test/reset")
        try await control("test/seed-report")
        try await control("test/enable-workflows")
        let app = XCUIApplication()
        app.launch()
        if app.tabBars.buttons["Data"].exists {
            tapTab(app, "Settings")
            app.buttons["Sign out and clear this device"].tap()
        }
        let server = app.textFields["https://health.example.com"]
        XCTAssertTrue(server.waitForExistence(timeout: 10))
        server.tap()
        let previous = server.value as? String ?? ""
        if previous != "https://health.example.com" { server.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: previous.count)) }
        server.typeText("http://127.0.0.1:18765")
        app.textFields["Username"].tap(); app.textFields["Username"].typeText("simulator")
        app.secureTextFields["Password"].tap(); app.secureTextFields["Password"].typeText("synthetic-test-password")
        app.buttons["Sign in"].tap()
        XCTAssertTrue(app.tabBars.buttons["Archive"].waitForExistence(timeout: 15))
        tapTab(app, "Archive")
        if app.buttons["Not Now"].waitForExistence(timeout: 3) { app.buttons["Not Now"].tap() }
        app.terminate(); app.launch()
        tapTab(app, "Archive")
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "fixture.png")).firstMatch.waitForExistence(timeout: 10))
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "fixture.png")).firstMatch.tap()
        XCTAssertTrue(app.buttons["Add result manually"].waitForExistence(timeout: 5))
        app.buttons["Add result manually"].tap()
        XCTAssertTrue(app.navigationBars["Add result"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Save"].isEnabled)
        let name = app.textFields["Name"].exists ? app.textFields["Name"] : app.textViews["Name"]
        name.tap(); name.typeText("Manual fixture result")
        let result = app.textFields["Result, including < or >"].exists ? app.textFields["Result, including < or >"] : app.textViews["Result, including < or >"]
        result.tap(); result.typeText("100")
        tapButton(app, "Save")
        XCTAssertTrue(app.navigationBars["Add result"].waitForNonExistence(timeout: 10))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Manual fixture result")).firstMatch.waitForExistence(timeout: 5))
        app.buttons.matching(identifier: "Revision history").firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Revision history"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Revision ")).firstMatch.waitForExistence(timeout: 5))
        app.navigationBars.buttons.firstMatch.tap()
        let evidence = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "text layer:")).firstMatch
        reveal(app, evidence)
        evidence.tap()
        XCTAssertTrue(app.staticTexts["Synthetic page text"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.firstMatch.tap()
        reveal(app, app.buttons["Page 1 · ocr"])
        app.buttons["Page 1 · ocr"].tap()
        XCTAssertTrue(app.staticTexts["Synthetic OCR output"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.firstMatch.tap()
        app.swipeDown()
        app.buttons["Duplicate or revised report"].tap()
        XCTAssertTrue(app.navigationBars["Prefer another report"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["other.png"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["This is a duplicate; prefer selected report"].waitForExistence(timeout: 5))
        app.buttons["This is a duplicate; prefer selected report"].tap()
        XCTAssertTrue(app.navigationBars["Prefer another report"].waitForNonExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Excluded from trends: duplicate. Original data is retained."].waitForExistence(timeout: 5))
        app.buttons["Treat as independent report"].tap()
        XCTAssertTrue(app.staticTexts["Excluded from trends: duplicate. Original data is retained."].waitForNonExistence(timeout: 10))
        tapTab(app, "Trends")
        let datedResult = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "2026-10-01")).firstMatch
        XCTAssertTrue(datedResult.waitForExistence(timeout: 10))
        reveal(app, datedResult)
        datedResult.tap()
        XCTAssertTrue(app.navigationBars["Original · page 1"].waitForExistence(timeout: 5))
        tapButton(app, "Done")
        XCTAssertTrue(app.navigationBars["Original · page 1"].waitForNonExistence(timeout: 5))
        tapTab(app, "Insights")
        XCTAssertTrue(app.buttons["Review lipid history and recent health data"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Review lipid history and recent health data"].isEnabled)
        app.buttons["Review lipid history and recent health data"].tap()
        let ready = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Ready")).firstMatch
        XCTAssertTrue(ready.waitForExistence(timeout: 10)); ready.tap()
        XCTAssertTrue(app.staticTexts["Synthetic lipid review"].waitForExistence(timeout: 5))
        let feedback = app.textFields["Incorrect, helpful, or discussed with your clinician"].exists ? app.textFields["Incorrect, helpful, or discussed with your clinician"] : app.textViews["Incorrect, helpful, or discussed with your clinician"]
        feedback.tap(); feedback.typeText("Synthetic feedback")
        app.buttons["Save feedback"].tap()
        XCTAssertTrue(app.staticTexts["Synthetic feedback"].waitForExistence(timeout: 10))
        app.navigationBars.buttons.firstMatch.tap()
        tapTab(app, "Settings")
        app.buttons["Create ZIP export"].tap()
        XCTAssertTrue(app.buttons["Delete export"].waitForExistence(timeout: 10))
        app.buttons["Delete export"].tap()
        XCTAssertTrue(app.buttons["Delete export"].waitForNonExistence(timeout: 10))
        app.buttons["Delete account and all server data"].tap()
        XCTAssertTrue(app.alerts["Delete your account?"].waitForExistence(timeout: 5))
        app.alerts.textFields.firstMatch.tap(); app.alerts.textFields.firstMatch.typeText("simulator")
        app.alerts.buttons["Delete"].tap()
        XCTAssertTrue(app.buttons["Sign in"].waitForExistence(timeout: 10))
    }
}
