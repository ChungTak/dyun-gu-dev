use std::sync::OnceLock;

use dg_graph::{
    CreatedElement, Element, ElementHandle, ElementIo, Error, NodeSpec, Packet, ParamField,
    ParamType, PortSchema, Result,
};

const INPUT_PORT: PortSchema = PortSchema {
    name: "in",
    dtype: None,
    required: true,
};
const HTTP_PUSH_FIELDS: &[&str] = &["url", "method"];
const HTTP_PUSH_PARAMS: &[ParamField] = &[
    ParamField {
        name: "url",
        ty: ParamType::Str,
        required: true,
    },
    ParamField {
        name: "method",
        ty: ParamType::Str,
        required: false,
    },
];

const MAX_URL_LENGTH: usize = 4096;
const MAX_METHOD_LENGTH: usize = 64;

pub struct HttpPushRequest {
    pub url: String,
    pub method: String,
    pub packet: Packet,
}

pub trait HttpPushDriver: Send + Sync {
    fn post(&self, request: HttpPushRequest) -> Result<()>;
}

static HTTP_PUSH_DRIVER: OnceLock<Box<dyn HttpPushDriver>> = OnceLock::new();

pub fn install_http_push_driver(driver: Box<dyn HttpPushDriver>) -> Result<()> {
    HTTP_PUSH_DRIVER
        .set(driver)
        .map_err(|_| Error::Config("http_push driver already installed".to_string()))
}

inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "http_push",
        input_ports: &[INPUT_PORT],
        output_ports: &[],
        params: HTTP_PUSH_PARAMS,
        validate: Some(validate_http_push),
        create: create_http_push,
    }
}

struct HttpPush {
    url: String,
    method: String,
}

impl Element for HttpPush {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        let redacted = redact_url(&self.url);
        loop {
            let packet = match io.recv("in")? {
                Some(packet) => packet,
                None => {
                    if io.should_stop() {
                        return Err(Error::NotRunning);
                    }
                    continue;
                }
            };
            if packet.is_eos() {
                return Ok(());
            }
            let driver = HTTP_PUSH_DRIVER.get().ok_or_else(|| {
                Error::Runtime(format!(
                    "http_push node {} has no installed driver for {redacted}",
                    io.name
                ))
            })?;
            driver
                .post(HttpPushRequest {
                    url: self.url.clone(),
                    method: self.method.clone(),
                    packet,
                })
                .map_err(|err| {
                    Error::Runtime(format!(
                        "http_push node {} failed posting to {}: {err}",
                        io.name, redacted
                    ))
                })?;
        }
    }
}

fn create_http_push(node: &NodeSpec) -> Result<CreatedElement> {
    let config = parse_http_push(node)?;
    Ok(CreatedElement {
        element: Box::new(HttpPush {
            url: config.url,
            method: config.method,
        }),
        handle: ElementHandle::None,
    })
}

fn validate_http_push(node: &NodeSpec) -> Result<()> {
    parse_http_push(node).map(|_| ())
}

struct HttpPushConfig {
    url: String,
    method: String,
}

fn parse_http_push(node: &NodeSpec) -> Result<HttpPushConfig> {
    let params = params_object(node)?;
    reject_unknown_fields(params, HTTP_PUSH_FIELDS)?;
    let url = match params.get("url") {
        Some(value) => value
            .as_str()
            .filter(|url| !url.is_empty())
            .ok_or_else(|| Error::Config("field url must be a non-empty string".to_string()))?,
        None => {
            return Err(Error::Config(
                "field url is required and must be a string".to_string(),
            ));
        }
    }
    .to_string();
    if url.len() > MAX_URL_LENGTH {
        return Err(Error::ResourceLimit {
            resource: "http_push url length".to_string(),
            requested: url.len(),
            limit: MAX_URL_LENGTH,
        });
    }
    validate_http_url(&url)?;
    let method = params
        .get("method")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| Error::Config("field method must be a string".to_string()))
        })
        .transpose()?
        .unwrap_or("POST");
    if method.is_empty() || method.chars().any(char::is_whitespace) {
        return Err(Error::Config(
            "field method must be a non-empty HTTP method".to_string(),
        ));
    }
    if method.len() > MAX_METHOD_LENGTH {
        return Err(Error::ResourceLimit {
            resource: "http_push method length".to_string(),
            requested: method.len(),
            limit: MAX_METHOD_LENGTH,
        });
    }
    Ok(HttpPushConfig {
        url,
        method: method.to_ascii_uppercase(),
    })
}

fn validate_http_url(url: &str) -> Result<()> {
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(Error::Config(
            "field url must use the http:// or https:// scheme".to_string(),
        ));
    };
    if (!scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https"))
        || rest.is_empty()
        || rest.chars().any(char::is_whitespace)
    {
        return Err(Error::Config(
            "field url must use a non-empty http:// or https:// URL".to_string(),
        ));
    }
    Ok(())
}

/// Redact userinfo, query and fragment from a URL so that error logs reveal the
/// scheme and host/path without leaking credentials or signed tokens.
fn redact_url(url: &str) -> String {
    let Some((scheme, after_scheme)) = url.split_once("://") else {
        return url.to_string();
    };
    let split_pos = after_scheme.find(['/', '?', '#']);
    let (authority, path_and_rest) = match split_pos {
        Some(pos) => {
            let (authority, rest) = after_scheme.split_at(pos);
            (authority, rest.to_string())
        }
        None => (after_scheme, String::new()),
    };
    let authority = authority
        .rsplit_once('@')
        .map(|(_, host_port)| host_port)
        .unwrap_or(authority);
    let path = path_and_rest
        .split_once('?')
        .map(|(path, _)| path.to_string())
        .or_else(|| {
            path_and_rest
                .split_once('#')
                .map(|(path, _)| path.to_string())
        })
        .unwrap_or(path_and_rest);
    format!("{scheme}://{authority}{path}")
}

fn params_object(node: &NodeSpec) -> Result<&serde_json::Map<String, serde_json::Value>> {
    node.params
        .as_object()
        .ok_or_else(|| Error::Config(format!("node {} params must be an object", node.name)))
}

fn reject_unknown_fields(
    params: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<()> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Error::Config(format!(
                "unknown field `{key}`; expected one of {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_url_strips_userinfo_and_query() {
        assert_eq!(
            redact_url("https://user:pass@example.com/path?token=secret"),
            "https://example.com/path"
        );
    }

    #[test]
    fn redact_url_leaves_no_query_url_intact() {
        assert_eq!(
            redact_url("http://example.com/stream"),
            "http://example.com/stream"
        );
    }

    #[test]
    fn redact_url_strips_query_and_fragment_without_path() {
        assert_eq!(
            redact_url("http://example.com?token=secret"),
            "http://example.com"
        );
        assert_eq!(
            redact_url("https://user:pass@example.com?token=secret#frag"),
            "https://example.com"
        );
    }
}
