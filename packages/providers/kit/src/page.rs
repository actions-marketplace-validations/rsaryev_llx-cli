//! The page the person's browser shows when signing in finishes: success or failure.

/// A loop: the mark of the page, drawn inline so the page needs nothing else.
const MARK: &str = "<svg viewBox=\"0 0 48 48\" width=\"56\" height=\"56\" aria-hidden=\"true\">\
    <path d=\"M36 14a15 15 0 1 0 3.6 15\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"3.5\" \
    stroke-linecap=\"round\"/><path d=\"M38 6v9h-9\" fill=\"none\" stroke=\"currentColor\" \
    stroke-width=\"3.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

const STYLE: &str = r#"
:root { --text: #fafafa; --text-dim: #a1a1aa; --page-bg: #09090b;
  --font-sans: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
* { box-sizing: border-box; }
html { color-scheme: dark; }
body { margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 24px;
  background: var(--page-bg); color: var(--text); font-family: var(--font-sans); text-align: center; }
main { width: 100%; max-width: 560px; display: flex; flex-direction: column; align-items: center; }
.mark { color: var(--text-dim); margin-bottom: 20px; }
h1 { margin: 0 0 10px; font-size: 28px; line-height: 1.15; font-weight: 650; }
p { margin: 0; line-height: 1.7; color: var(--text-dim); font-size: 15px; }
.details { margin-top: 16px; font-family: var(--font-mono); font-size: 13px; color: var(--text-dim);
  white-space: pre-wrap; word-break: break-word; }
"#;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn page(heading: &str, message: &str, details: Option<&str>) -> String {
    let details = details
        .map(|details| format!("<div class=\"details\">{}</div>", escape(details)))
        .unwrap_or_default();
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\
         <title>{heading}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n\
         <div class=\"mark\">{MARK}</div>\n<h1>{heading}</h1>\n<p>{message}</p>\n{details}\n\
         </main>\n</body>\n</html>",
        heading = escape(heading),
        message = escape(message),
    )
}

/// `app` came back from signing in to `provider`.
pub(crate) fn success(app: &str, provider: &str) -> String {
    page(
        "Authentication successful",
        &format!(
            "{app} is signing in to {provider}. You can close this tab and return to the terminal."
        ),
        None,
    )
}

pub(crate) fn failure(message: &str, details: Option<&str>) -> String {
    page("Authentication failed", message, details)
}

#[cfg(test)]
#[path = "page_tests.rs"]
mod tests;
