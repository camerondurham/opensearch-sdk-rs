use crate::rest::{ExtensionRestRequest, ExtensionRestResponse, RestMethod, RestStatus};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

pub type RouteHandler = Arc<dyn Fn(ExtensionRestRequest) -> ExtensionRestResponse + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionMetadata {
    pub name: String,
    pub unique_id: String,
    pub host_address: IpAddr,
    pub port: u16,
    pub version: String,
    pub opensearch_version: String,
    pub minimum_compatible_version: String,
}

impl ExtensionMetadata {
    pub fn new(name: impl Into<String>, unique_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            unique_id: unique_id.into(),
            host_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 1234,
            version: "0.1.0".into(),
            opensearch_version: "3.6.0".into(),
            minimum_compatible_version: "2.19.0".into(),
        }
    }
}

#[derive(Clone)]
pub struct Route {
    pub method: RestMethod,
    pub path: String,
    pub name: String,
    pub handler: RouteHandler,
}

impl Route {
    pub fn new<F>(
        method: RestMethod,
        path: impl Into<String>,
        name: impl Into<String>,
        handler: F,
    ) -> Self
    where
        F: Fn(ExtensionRestRequest) -> ExtensionRestResponse + Send + Sync + 'static,
    {
        Self {
            method,
            path: path.into(),
            name: name.into(),
            handler: Arc::new(handler),
        }
    }

    pub fn registration_string(&self) -> String {
        format!("{} {} {}", self.method.as_str(), self.path, self.name)
    }

    pub fn route_key(&self) -> String {
        format!("{} {}", self.method.as_str(), self.path)
    }

    pub fn matches(&self, method: RestMethod, path: &str) -> bool {
        if self.method != method {
            return false;
        }

        let expected = self.path.split('/').filter(|segment| !segment.is_empty());
        let actual = path.split('/').filter(|segment| !segment.is_empty());

        let expected = expected.collect::<Vec<_>>();
        let actual = actual.collect::<Vec<_>>();

        if expected.len() != actual.len() {
            return false;
        }

        expected
            .iter()
            .zip(actual.iter())
            .all(|(expected, actual)| {
                (expected.starts_with('{') && expected.ends_with('}')) || expected == actual
            })
    }
}

pub trait Extension: Send + Sync + 'static {
    fn metadata(&self) -> &ExtensionMetadata;
    fn routes(&self) -> Vec<Route>;

    fn implemented_interfaces(&self) -> Vec<String> {
        if self.routes().is_empty() {
            Vec::new()
        } else {
            vec!["ActionExtension".into()]
        }
    }
}

pub fn not_found_response(request: ExtensionRestRequest) -> ExtensionRestResponse {
    ExtensionRestResponse::from_request(
        request,
        RestStatus::NotFound,
        ExtensionRestResponse::TEXT_CONTENT_TYPE,
        b"No handler for route".to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::Route;
    use crate::rest::{
        ExtensionRestRequest, ExtensionRestResponse, HttpVersion, RestMethod, RestStatus,
    };
    use std::collections::BTreeMap;

    fn request(path: &str) -> ExtensionRestRequest {
        ExtensionRestRequest::new(
            RestMethod::Get,
            path.into(),
            path.into(),
            BTreeMap::new(),
            BTreeMap::new(),
            None,
            Vec::new(),
            String::new(),
            HttpVersion::Http11,
        )
    }

    #[test]
    fn route_matches_exact_and_named_wildcards() {
        let route = Route::new(RestMethod::Get, "/hello/{name}", "test:hello", |request| {
            ExtensionRestResponse::from_request(
                request,
                RestStatus::Ok,
                ExtensionRestResponse::TEXT_CONTENT_TYPE,
                b"ok".to_vec(),
            )
        });

        assert!(route.matches(RestMethod::Get, "/hello/world"));
        assert!(!route.matches(RestMethod::Post, "/hello/world"));
        assert!(!route.matches(RestMethod::Get, "/hello"));

        let response = (route.handler)(request("/hello/world"));
        assert_eq!(response.status, RestStatus::Ok);
    }
}
