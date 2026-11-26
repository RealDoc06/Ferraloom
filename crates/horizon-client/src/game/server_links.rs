/// `Util.parseAndValidateUntrustedUri`: an absolute `http`/`https` URI
/// (java.net.URI's scheme then the allowlist; whitespace-free).
pub(super) fn valid_untrusted_uri(uri: &str) -> bool {
    let Some(colon) = uri.find(':') else {
        return false;
    };
    let scheme = &uri[..colon];
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    !uri.chars().any(|c| c.is_control() || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_uris_are_http_or_https_with_a_scheme() {
        for ok in [
            "https://example.net/x",
            "http://example.net",
            "HTTPS://EXAMPLE.NET",
        ] {
            assert!(valid_untrusted_uri(ok), "{ok}");
        }
        for bad in [
            "example.net/no-scheme",
            "ftp://example.net",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://exa mple.net",
            "://example.net",
            "",
        ] {
            assert!(!valid_untrusted_uri(bad), "{bad}");
        }
    }
}
