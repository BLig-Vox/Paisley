#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    Home,
    Url(String),
}

impl Navigation {
    pub fn uri(&self) -> &str {
        match self {
            Self::Home => "https://duckduckgo.com/",
            Self::Url(url) => url,
        }
    }
}

/// Resolve submitted omnibox text without performing network requests.
pub fn resolve(input: &str) -> Result<Navigation, String> {
    let input = input.trim();
    if input.is_empty() || matches!(input, "paisley:home" | "browny:home" | "about:blank") {
        return Ok(Navigation::Home);
    }
    if input.chars().any(char::is_control) {
        return Err("Addresses cannot contain control characters.".into());
    }

    let mut has_port = false;
    if let Some((scheme, rest)) = input.split_once(':') {
        let valid_scheme = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if valid_scheme {
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                if !rest.starts_with("//") {
                    return Err("Use a complete address beginning with http:// or https://.".into());
                }
                return web_url(input);
            }
            let port = rest.split(['/', '?', '#']).next().unwrap_or("");
            has_port = !port.is_empty() && port.chars().all(|c| c.is_ascii_digit());
            if !has_port {
                return Err("Only HTTP and HTTPS website addresses are supported.".into());
            }
        }
    }

    let authority = input.split(['/', '?', '#']).next().unwrap_or("");
    let looks_like_host = !input.chars().any(char::is_whitespace)
        && (authority.contains('.')
            || authority.starts_with('[')
            || authority == "localhost"
            || has_port);

    if input.starts_with("//") {
        return web_url(&format!("https:{input}"));
    }
    if looks_like_host {
        let mut url = url::Url::parse(&format!("https://{input}"))
            .map_err(|_| "That website address is not valid.".to_owned())?;
        let loopback = match url.host() {
            Some(url::Host::Domain(host)) => host == "localhost" || host.ends_with(".localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if loopback {
            let _ = url.set_scheme("http");
        }
        return web_url(url.as_str());
    }

    let mut search = url::Url::parse("https://duckduckgo.com/").expect("static search URL");
    search.query_pairs_mut().append_pair("q", input);
    Ok(Navigation::Url(search.into()))
}

/// A loaded page can create embedded documents that are not network URLs.
/// Let WebKit enforce their normal origin and sandbox restrictions; the
/// address bar deliberately remains limited to HTTP/HTTPS via `resolve`.
pub fn is_web_content_uri(uri: &str) -> bool {
    let Ok(url) = url::Url::parse(uri) else {
        return false;
    };
    match url.scheme() {
        "http" | "https" => url.host_str().is_some(),
        "about" => matches!(url.path(), "blank" | "srcdoc"),
        "blob" | "data" => true,
        _ => false,
    }
}

fn web_url(input: &str) -> Result<Navigation, String> {
    let url =
        url::Url::parse(input).map_err(|_| "That website address is not valid.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Enter a website address beginning with http:// or https://.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Addresses containing a username or password are not supported.".into());
    }
    Ok(Navigation::Url(url.into()))
}

#[cfg(test)]
mod tests {
    use super::{Navigation, is_web_content_uri, resolve};

    #[test]
    fn page_navigation_allows_engine_managed_documents() {
        for uri in [
            "https://duck.ai/chat?ia=chat&duckai=1&home=1&prompt=1&origin=funnel_home_website&t=h_",
            "http://localhost:3000/",
            "about:blank",
            "about:blank#frame",
            "about:srcdoc",
            "blob:https://duck.ai/79f541e4-5e33-4e0a-916d-2a4d1d0c3715",
            "data:text/html,Hello%20world",
        ] {
            assert!(is_web_content_uri(uri), "blocked web content {uri}");
        }
    }

    #[test]
    fn page_navigation_still_blocks_local_files_and_external_protocols() {
        for uri in [
            "file:///etc/passwd",
            "mailto:hello@example.org",
            "javascript:alert(1)",
            "custom:thing",
            "about:config",
            "about:settings",
            "not a URI",
            "https://",
        ] {
            assert!(
                !is_web_content_uri(uri),
                "accepted unsupported navigation {uri}"
            );
        }
    }

    #[test]
    fn home_addresses_resolve_to_duckduckgo() {
        for input in ["", "  ", "paisley:home", "about:blank"] {
            let navigation = resolve(input).expect("valid home address");
            assert_eq!(navigation.uri(), "https://duckduckgo.com/", "{input:?}");
        }
    }

    #[test]
    fn website_destinations_preserve_the_resolved_url() {
        let navigation = resolve("example.com/path").expect("valid website");
        assert_eq!(navigation.uri(), "https://example.com/path");
    }

    #[test]
    fn public_hostnames_default_to_https() {
        for (input, expected) in [
            ("example.com", "https://example.com/"),
            (
                "  example.com/path?q=hello#part  ",
                "https://example.com/path?q=hello#part",
            ),
            ("example.com:8443", "https://example.com:8443/"),
            ("//example.com", "https://example.com/"),
        ] {
            assert_eq!(resolve(input), Ok(Navigation::Url(expected.into())));
        }
    }

    #[test]
    fn loopback_development_addresses_default_to_http() {
        for (input, expected) in [
            ("localhost:3000/path", "http://localhost:3000/path"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080/"),
            ("127.0.0.2", "http://127.0.0.2/"),
            ("[::1]:8000", "http://[::1]:8000/"),
        ] {
            assert_eq!(resolve(input), Ok(Navigation::Url(expected.into())));
        }
    }

    #[test]
    fn explicit_web_urls_preserve_their_scheme_and_components() {
        for (input, expected) in [
            ("http://example.com/a?q=1#b", "http://example.com/a?q=1#b"),
            ("HTTPS://Example.COM", "https://example.com/"),
            ("https://localhost:4443", "https://localhost:4443/"),
            ("https://bücher.de", "https://xn--bcher-kva.de/"),
        ] {
            assert_eq!(resolve(input), Ok(Navigation::Url(expected.into())));
        }
    }

    #[test]
    fn search_queries_are_encoded_without_injecting_parameters() {
        assert_eq!(
            resolve("rust & gtk"),
            Ok(Navigation::Url(
                "https://duckduckgo.com/?q=rust+%26+gtk".into()
            ))
        );
        assert_eq!(
            resolve("rust"),
            Ok(Navigation::Url("https://duckduckgo.com/?q=rust".into()))
        );
        assert_eq!(
            resolve("what is example.com"),
            Ok(Navigation::Url(
                "https://duckduckgo.com/?q=what+is+example.com".into()
            ))
        );
    }

    #[test]
    fn unsupported_schemes_are_rejected_instead_of_executed_or_searched() {
        for input in [
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "ftp://example.com",
            "mailto:test@example.com",
            "about:config",
            "paisley:settings",
            "custom:thing",
        ] {
            assert!(resolve(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn malformed_addresses_and_control_characters_are_rejected() {
        for input in [
            "https://",
            "http://",
            "https://exa mple.com",
            "http://[::1",
            "example.com:99999",
            "http://example.com\n.evil.test",
            "javascript:\talert(1)",
            "https:example.com",
            "http:/example.com",
            "https://user:secret@example.com",
        ] {
            assert!(resolve(input).is_err(), "accepted {input:?}");
        }
    }
}
