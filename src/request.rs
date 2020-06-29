use std::fmt::Debug;

#[cfg(feature = "blocking")]
use reqwest::blocking::Client as ReqwestClient;
#[cfg(not(feature = "blocking"))]
use reqwest::Client as ReqwestClient;

use reqwest::{
    header::{HeaderName, HeaderValue, CONTENT_TYPE},
    Method, Url,
};
use serde::Serialize;
use uuid::Uuid;

use crate::errors::{BridgeRsError, BridgeRsResult};
use crate::prelude::*;

pub struct Request<'a, S: Serialize> {
    bridge: &'a Bridge,
    request_type: RequestType<S>,
    custom_headers: Vec<(HeaderName, HeaderValue)>,
    path: Option<&'a str>,
    query_pairs: Vec<(&'a str, &'a str)>,
}

impl<'a, S: Serialize> Request<'a, S> {
    pub fn new(bridge: &'a Bridge, request_type: RequestType<S>) -> Self {
        Self {
            bridge,
            request_type,
            custom_headers: vec![],
            path: None,
            query_pairs: vec![],
        }
    }

    pub fn with_custom_headers(self, headers: Vec<(HeaderName, HeaderValue)>) -> Self {
        Self {
            custom_headers: headers,
            ..self
        }
    }

    pub fn to(self, path: &'a str) -> Self {
        Self {
            path: Some(path),
            ..self
        }
    }

    pub fn with_query_pair(self, name: &'a str, value: &'a str) -> Self {
        let mut query_pairs = self.query_pairs;
        query_pairs.push((name, value));
        Self {
            query_pairs,
            ..self
        }
    }

    /// issue the external request by consuming the bridge request and
    /// returning a [Response](struct.Response.html)
    #[cfg(feature = "blocking")]
    pub fn send(self) -> BridgeRsResult<Response> {
        let request = self.get_request_type();

        let request_builder = self
            .get_client()
            .request(request.get_method(), self.get_url().as_str())
            .header(CONTENT_TYPE, "application/json")
            .header(
                HeaderName::from_static("x-request-id"),
                &request.id().to_string(),
            );
        let request_builder = self
            .custom_headers()
            .iter()
            .fold(request_builder, |request, (name, value)| {
                request.header(name, value)
            });

        let response = request_builder
            .body(request.body_as_string()?)
            .send()
            .map_err(|e| BridgeRsError::HttpError {
                url: self.get_url(),
                source: e,
            })?;
        let status_code = response.status();
        if !status_code.is_success() {
            return Err(BridgeRsError::WrongStatusCode(self.get_url(), status_code));
        }
        let response_body = response.text().map_err(|e| BridgeRsError::HttpError {
            source: e,
            url: self.get_url(),
        })?;
        let response = match request {
            RequestType::GraphQL(_) => Response::graphql(
                self.get_url(),
                response_body,
                status_code,
                self.get_request_type().id(),
            ),
            RequestType::Rest(_) => Response::rest(
                self.get_url(),
                response_body,
                status_code,
                self.get_request_type().id(),
            ),
        };
        Ok(response)
    }

    #[cfg(not(feature = "blocking"))]
    pub fn send(self) -> impl std::future::Future<Output = BridgeRsResult<Response>> + 'a
    where
        Self: 'a,
    {
        use futures::future::FutureExt;
        use futures_util::future::TryFutureExt;
        let url = self.get_url();
        let url2 = url.clone();
        let url3 = url.clone();
        let url4 = url.clone();
        let request_id = self.get_request_type().id();

        let request_builder = self
            .get_client()
            .request(
                self.get_request_type().get_method(),
                self.get_url().as_str(),
            )
            .header(CONTENT_TYPE, "application/json")
            .header(
                HeaderName::from_static("x-request-id"),
                &request_id.to_string(),
            );
        let request_builder = self
            .custom_headers()
            .iter()
            .fold(request_builder, |request, (name, value)| {
                request.header(name, value)
            });

        let body_as_string = match self.get_request_type().body_as_string() {
            Ok(body_as_string) => body_as_string,
            Err(e) => {
                return async { Err(e) }.right_future();
            }
        };

        request_builder
            .body(body_as_string)
            .send()
            .map_err(|e| BridgeRsError::HttpError { url, source: e })
            .and_then(|response| {
                let status_code = response.status();
                async move {
                    if status_code.is_success() {
                        Ok((response.status(), response))
                    } else {
                        Err(BridgeRsError::WrongStatusCode(url2, status_code))
                    }
                }
            })
            .and_then(move |(status_code, response)| {
                response
                    .text()
                    .map_err(|e| BridgeRsError::HttpError {
                        source: e,
                        url: url3,
                    })
                    .map_ok(move |response_body| {
                        if self.get_request_type().is_graphql() {
                            Response::graphql(url4, response_body, status_code, request_id)
                        } else {
                            Response::rest(url4, response_body, status_code, request_id)
                        }
                    })
            })
            .left_future()
    }

    fn get_client(&self) -> &ReqwestClient {
        &self.bridge.client
    }

    fn get_request_type(&self) -> &RequestType<S> {
        &self.request_type
    }

    fn custom_headers(&self) -> &[(HeaderName, HeaderValue)] {
        &self.custom_headers
    }

    fn get_path(&self) -> Option<&str> {
        self.path
    }

    fn get_url(&self) -> Url {
        let mut endpoint = self.bridge.endpoint.clone();
        let path_segments = self.bridge.endpoint.path_segments();
        let path = self.get_path();

        endpoint = match path {
            Some(path) => {
                let mut parts: Vec<&str> = path_segments
                    .map_or_else(Vec::new, |ps| ps.collect())
                    .into_iter()
                    .filter(|p| p != &"")
                    .collect();
                parts.push(path);
                endpoint.set_path(&parts.join("/"));
                endpoint
            }
            _ => endpoint,
        };

        self.query_pairs
            .iter()
            .fold(endpoint, |mut url, (name, value)| {
                url.query_pairs_mut().append_pair(name, value);
                url
            })
    }
}

#[derive(Debug)]
pub enum RequestType<S: Serialize> {
    GraphQL(GraphQL<S>),
    Rest(Rest<S>),
}

#[derive(Debug)]
pub struct GraphQL<S: Serialize> {
    request_id: Uuid,
    body: GraphQLBody<S>,
}

#[derive(Debug, Serialize)]
pub struct GraphQLBody<T> {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<T>,
}

#[derive(Debug)]
pub struct Rest<S: Serialize> {
    request_id: Uuid,
    body: Option<RestBody<S>>,
    method: Method,
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct RestBody<S: Serialize> {
    value: S,
}

impl<S: Serialize> RequestType<S> {
    pub fn id(&self) -> Uuid {
        match self {
            RequestType::GraphQL(request) => request.request_id,
            RequestType::Rest(request) => request.request_id,
        }
    }

    pub fn body_as_string(&self) -> BridgeRsResult<String> {
        match self {
            RequestType::GraphQL(request) => Ok(serde_json::to_string(&request.body)?),
            RequestType::Rest(request) => Ok(serde_json::to_string(&request.body)?),
        }
    }

    pub fn is_graphql(&self) -> bool {
        match self {
            RequestType::GraphQL(_) => true,
            RequestType::Rest(_) => false,
        }
    }

    pub fn is_rest(&self) -> bool {
        match self {
            RequestType::GraphQL(_) => false,
            RequestType::Rest(_) => true,
        }
    }

    pub fn get_method(&self) -> Method {
        match self {
            RequestType::GraphQL(_) => Method::POST,
            RequestType::Rest(request) => request.method.clone(),
        }
    }

    pub fn rest(body: Option<S>, method: Method) -> Self {
        let request_id = Uuid::new_v4();
        match body {
            None => Self::Rest(Rest {
                request_id,
                method,
                body: None,
            }),
            Some(body) => Self::Rest(Rest {
                request_id,
                body: Some(RestBody { value: body }),
                method,
            }),
        }
    }

    pub fn graphql(query: &str, variables: Option<S>) -> Self {
        Self::GraphQL(GraphQL {
            request_id: Uuid::new_v4(),
            body: GraphQLBody {
                query: query.to_string(),
                variables,
            },
        })
    }
}