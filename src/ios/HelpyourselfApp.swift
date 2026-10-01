import SwiftUI

@main
struct HelpyourselfApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var phase
    var body: some Scene {
        WindowGroup {
            RootView(model: model)
                .tint(.teal)
                .onChange(of: phase) { _, next in
                    if next == .active { Task { await model.sendDrafts(); await model.refresh(); await model.synchronizeHealth(requestAccess: false) } }
                }
        }
    }
}

struct RootView: View {
    @Bindable var model: AppModel
    var body: some View {
        Group {
            if model.session == nil { LoginView(model: model) }
            else {
                TabView {
                    NavigationStack { ReportsView(model: model) }.tabItem { Label("Reports", systemImage: "doc.text") }
                    NavigationStack { TrendsView(model: model) }.tabItem { Label("Trends", systemImage: "chart.xyaxis.line") }
                    NavigationStack { HealthView(model: model) }.tabItem { Label("Health", systemImage: "heart") }
                    NavigationStack { AnalysisView(model: model) }.tabItem { Label("Insights", systemImage: "sparkles") }
                    NavigationStack { SettingsView(model: model) }.tabItem { Label("Settings", systemImage: "gearshape") }
                }
                .task { await model.sendDrafts(); await model.refresh(); await model.synchronizeHealth(requestAccess: false) }
            }
        }
        .overlay(alignment: .top) { if model.isBusy { ProgressView("Working…").padding(10).background(.regularMaterial, in: Capsule()).accessibilityLabel("Processing your request") } }
        .alert("Could not complete the action", isPresented: Binding(get: { model.errorMessage != nil }, set: { if !$0 { model.errorMessage = nil } })) {
            Button("OK") { model.errorMessage = nil }
        } message: { Text(model.errorMessage ?? "") }
    }
}

private struct LoginView: View {
    var model: AppModel
    @State private var server = UserDefaults.standard.string(forKey: "lastServer") ?? ""
    @State private var username = ""
    @State private var password = ""
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Image(systemName: "heart.text.clipboard").font(.system(size: 44)).foregroundStyle(.teal)
                    Text("Your health. Your history.").font(.title2.bold())
                    Text("Connect to your own helpyourself server.").foregroundStyle(.secondary)
                }
                Section("Server") { TextField("https://health.example.com", text: $server).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled() }
                Section("Account") {
                    TextField("Username", text: $username).textContentType(.username).textInputAutocapitalization(.never).autocorrectionDisabled()
                    SecureField("Password", text: $password).textContentType(.password)
                }
                Button {
                    Task { await model.login(server: server, username: username, password: password); password = "" }
                } label: { if model.isBusy { ProgressView() } else { Text("Sign in") } }
                .disabled(model.isBusy || server.isEmpty || username.isEmpty || password.isEmpty)
                Section { Text("Accounts are created by your server administrator. Reports and AI processing use the services configured on that server.").font(.footnote).foregroundStyle(.secondary) }
            }.navigationTitle("helpyourself")
        }
    }
}
