//! Web pages, rendered from the templates in `src/templates/`.
//!
//! Jinja emits the text between its tags verbatim, so each function below is the
//! template text with the tags replaced by the equivalent Rust control flow.
//! `conformance/` checks the output against the captured Python HTML byte for byte.

use crate::config::Config;

/// MarkupSafe's HTML escape with `autoescape=True`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// `base.html` with the `content` block of a page substituted in.
pub fn base(messages: &[(String, String)], content: &str) -> String {
    let mut out = String::new();
    out.push_str(r#"<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Octopus MinMax Bot</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {
            darkMode: 'class',
        }
    </script>
</head>
<body class="bg-gray-900 text-gray-100 min-h-screen">
    <nav class="bg-gray-800 border-b border-gray-700 px-6 py-4">
        <div class="max-w-7xl mx-auto">
            <h1 class="text-2xl font-bold text-blue-400">🐙 Octopus MinMax Bot</h1>
        </div>
    </nav>

    <main class="max-w-7xl mx-auto px-6 py-8">
        <!-- Flash messages -->
        "#);
    out.push_str(r#"
            "#);
    if !messages.is_empty() {
        out.push_str(r#"
                "#);
        for (category, message) in messages {
            out.push_str(r#"
                    <div class="mb-6 p-4 rounded-lg "#);
            if category == "error" {
                out.push_str(r#"bg-red-900 border border-red-700 text-red-100"#);
            } else {
                out.push_str(r#"bg-green-900 border border-green-700 text-green-100"#);
            }
            out.push_str(r#"">
                        "#);
            out.push_str(&escape(message));
            out.push_str(r#"
                    </div>
                "#);
        }
        out.push_str(r#"
            "#);
    }
    out.push_str(r#"
        "#);
    out.push_str(r#"

        "#);
    out.push_str(content);
    out.push_str(r#"
    </main>
</body>
</html>"#);
    out
}

pub fn index_page() -> String {
    let mut out = String::new();
    out.push_str(r#"
<div class="flex flex-col items-center justify-center min-h-[60vh]">
    <h2 class="text-3xl font-bold mb-12 text-gray-100">Dashboard</h2>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-8 w-full max-w-4xl">
        <!-- Config Button -->
        <a href="config" class="group">
            <div class="bg-gray-800 hover:bg-gray-700 border-2 border-gray-600 hover:border-blue-500 rounded-2xl p-12 text-center transition-all duration-200 transform hover:scale-105">
                <div class="text-6xl mb-4">⚙️</div>
                <h3 class="text-2xl font-bold text-gray-100 group-hover:text-blue-400">Configuration</h3>
                <p class="text-gray-400 mt-2">Edit bot settings</p>
            </div>
        </a>

        <!-- Logs Button -->
        <a href="logs" class="group">
            <div class="bg-gray-800 hover:bg-gray-700 border-2 border-gray-600 hover:border-blue-500 rounded-2xl p-12 text-center transition-all duration-200 transform hover:scale-105">
                <div class="text-6xl mb-4">📋</div>
                <h3 class="text-2xl font-bold text-gray-100 group-hover:text-blue-400">Logs</h3>
                <p class="text-gray-400 mt-2">View bot activity</p>
            </div>
        </a>
    </div>
</div>
"#);
    out
}

pub fn config_page(cfg: &Config) -> String {
    let mut out = String::new();
    out.push_str(r#"
<div class="max-w-4xl mx-auto">
    <div class="flex justify-between items-center mb-6">
        <h2 class="text-3xl font-bold text-gray-100">Configuration</h2>
        <a href="." class="text-blue-400 hover:text-blue-300">← Back to Dashboard</a>
    </div>

    <div class="bg-yellow-900 border border-yellow-700 text-yellow-100 rounded-lg p-4 mb-6">
        ⚠️ <strong>Note:</strong> Config changes apply immediately but reset on container restart. Update docker-compose.yaml for permanent changes.
    </div>

    <form method="POST" class="space-y-6">
        <!-- API Settings -->
        <div class="bg-gray-800 border border-gray-700 rounded-lg p-6">
            <h3 class="text-xl font-bold text-gray-100 mb-4">API Settings</h3>

            <div class="space-y-4">
                <div>
                    <label class="block text-gray-300 mb-2" for="api_key">API Key</label>
                    <input type="text" id="api_key" name="api_key" value=""#);
    out.push_str(&escape(&cfg.api_key));
    out.push_str(r#""
                           class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                </div>

                <div>
                    <label class="block text-gray-300 mb-2" for="acc_number">Account Number</label>
                    <input type="text" id="acc_number" name="acc_number" value=""#);
    out.push_str(&escape(&cfg.acc_number));
    out.push_str(r#""
                           class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                </div>

                <div>
                    <label class="block text-gray-300 mb-2" for="base_url">Base URL</label>
                    <input type="text" id="base_url" name="base_url" value=""#);
    out.push_str(&escape(&cfg.base_url));
    out.push_str(r#""
                           class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                </div>
            </div>
        </div>

        <!-- Execution Settings -->
        <div class="bg-gray-800 border border-gray-700 rounded-lg p-6">
            <h3 class="text-xl font-bold text-gray-100 mb-4">Execution Settings</h3>

            <div class="space-y-4">
                <div>
                    <label class="block text-gray-300 mb-2" for="execution_time">Execution Time (HH:MM)</label>
                    <input type="text" id="execution_time" name="execution_time" value=""#);
    out.push_str(&escape(&cfg.execution_time));
    out.push_str(r#""
                           placeholder="23:00"
                           class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                </div>

                <div>
                    <label class="block text-gray-300 mb-2" for="switch_threshold">Switch Threshold (pence)</label>
                    <input type="number" id="switch_threshold" name="switch_threshold" value=""#);
    out.push_str(&escape(&cfg.switch_threshold.to_string()));
    out.push_str(r#""
                           class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                </div>

                <div class="flex items-center">
                    <input type="checkbox" id="dry_run" name="dry_run" "#);
    if cfg.dry_run {
        out.push_str(r#"checked"#);
    }
    out.push_str(r#"
                           class="w-5 h-5 bg-gray-900 border border-gray-600 rounded">
                    <label class="ml-3 text-gray-300" for="dry_run">Dry Run (compare only, don't switch)</label>
                </div>
                <div class="flex items-center">
                    <input type="checkbox" id="one_off_run" name="one_off_run" "#);
    if cfg.one_off_run {
        out.push_str(r#"checked"#);
    }
    out.push_str(r#"
                           class="w-5 h-5 bg-gray-900 border border-gray-600 rounded">
                    <label class="ml-3 text-gray-300" for="one_off_run">One-Off Run (run once and wait)</label>
                </div>
            </div>
        </div>

        <!-- Tariff Settings -->
        <div class="bg-gray-800 border border-gray-700 rounded-lg p-6">
            <h3 class="text-xl font-bold text-gray-100 mb-4">Tariff Settings</h3>

            <div>
                <label class="block text-gray-300 mb-2" for="tariffs">Tariffs (comma-separated)</label>
                <input type="text" id="tariffs" name="tariffs" value=""#);
    out.push_str(&escape(&cfg.tariffs));
    out.push_str(r#""
                       placeholder="go,agile,flexible"
                       class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">
                <p class="text-gray-500 text-sm mt-2">Valid: go, agile, cosy, flexible</p>
            </div>
        </div>

        <!-- Notification Settings -->
        <div class="bg-gray-800 border border-gray-700 rounded-lg p-6">
            <h3 class="text-xl font-bold text-gray-100 mb-4">Notification Settings</h3>

            <div class="space-y-4">
                <div>
                    <label class="block text-gray-300 mb-2" for="notification_urls">Notification URLs (Apprise)</label>
                    <textarea id="notification_urls" name="notification_urls" rows="3"
                              class="w-full bg-gray-900 border border-gray-600 text-gray-100 rounded px-4 py-2 focus:outline-none focus:border-blue-500">"#);
    out.push_str(&escape(&cfg.notification_urls));
    out.push_str(r#"</textarea>
                    <p class="text-gray-500 text-sm mt-2">Comma-separated Apprise URLs</p>
                </div>

                <div class="flex items-center">
                    <input type="checkbox" id="batch_notifications" name="batch_notifications" "#);
    if cfg.batch_notifications {
        out.push_str(r#"checked"#);
    }
    out.push_str(r#"
                           class="w-5 h-5 bg-gray-900 border border-gray-600 rounded">
                    <label class="ml-3 text-gray-300" for="batch_notifications">Batch Notifications</label>
                </div>
            </div>
        </div>

        <!-- Submit Button -->
        <div class="flex justify-end">
            <button type="submit"
                    class="bg-blue-600 hover:bg-blue-700 text-white font-bold py-3 px-8 rounded-lg transition-colors duration-200">
                Update Configuration
            </button>
        </div>
    </form>
</div>
"#);
    out
}

pub fn logs_page(entries: &[String]) -> String {
    let mut out = String::new();
    out.push_str(r#"
<div class="max-w-6xl mx-auto">
    <div class="flex justify-between items-center mb-6">
        <h2 class="text-3xl font-bold text-gray-100">Bot Logs</h2>
        <a href="." class="text-blue-400 hover:text-blue-300">← Back to Dashboard</a>
    </div>

    <div class="bg-gray-800 border border-gray-700 rounded-lg p-6 overflow-hidden">
        <div id="log-container" class="bg-black rounded p-4 overflow-y-auto" style="max-height: 70vh;">
            <div class="text-sm font-mono text-gray-300">
                "#);
    for item in entries {
        out.push_str(r#"
                    <div class="border-b border-gray-800 pb-2 mb-2 whitespace-pre-wrap">"#);
        out.push_str(&escape(item));
        out.push_str(r#"</div>
                "#);
    }
    out.push_str(r#"
            </div>
        </div>
    </div>
</div>

<script>
    // Scroll to bottom on page load
    window.addEventListener('load', function() {
        const logContainer = document.getElementById('log-container');
        logContainer.scrollTop = logContainer.scrollHeight;
    });
</script>
"#);
    out
}
